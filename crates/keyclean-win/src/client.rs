//! The app's handle to the engine process (ADR 0009).
//!
//! Starts `keyclean.exe --engine --parent <app pid>` (see `host`), sends it commands on its stdin
//! and turns the event lines on its stdout back into [`EngineEvent`]s. Same API as [`Engine`], so
//! the app doesn't care where the engine runs.
//!
//! [`Engine`]: crate::Engine

use std::io::{BufRead, BufReader, Read, Write};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use keyclean_core::policy::SafetyProfile;
use keyclean_core::session::{EndReason, SessionState};
use windows::Win32::System::Threading::CREATE_NO_WINDOW;

use crate::devices::{self, KeyboardDevice};
use crate::engine::{EngineEvent, EngineStatus, LockRequest, safety_profile};
use crate::error::EngineError;
use crate::host::{ENGINE_FLAG, PARENT_FLAG};
use crate::protocol::{WireCommand, WireEvent, to_line};

/// How long the engine process has to report that it is ready.
const READY_TIMEOUT: Duration = Duration::from_secs(5);
/// How long `Drop` waits for the engine process to exit before killing it. Longer than the
/// engine's own shutdown wait (3 s), so the kill only hits a process that is truly stuck.
const EXIT_WAIT: Duration = Duration::from_secs(5);
const EXIT_POLL: Duration = Duration::from_millis(10);

/// Handle to the engine process. Dropping it releases any lock and stops the process.
pub struct EngineClient {
    /// `None` once closed.
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Child>,
    /// Set when the app stops the engine on purpose, so its exit isn't reported as an error.
    closing: Arc<AtomicBool>,
}

impl EngineClient {
    /// Starts the engine process from this executable and waits until it is ready.
    pub fn start() -> Result<(EngineClient, Receiver<EngineEvent>), EngineError> {
        let exe = std::env::current_exe()
            .map_err(|e| EngineError::EngineProcess(format!("can't locate the executable: {e}")))?;
        Self::start_exe(&exe)
    }

    /// Testkit only: starts the engine process from `exe` (the app's executable), so the
    /// end-to-end harness can drive the real process and pipe path.
    #[cfg(feature = "testkit")]
    pub fn start_from(exe: &Path) -> Result<(EngineClient, Receiver<EngineEvent>), EngineError> {
        Self::start_exe(exe)
    }

    fn start_exe(exe: &Path) -> Result<(EngineClient, Receiver<EngineEvent>), EngineError> {
        // Release builds have no console, so there is no stderr to pass on.
        let stderr = if cfg!(debug_assertions) {
            Stdio::inherit()
        } else {
            Stdio::null()
        };
        let mut child = Command::new(exe)
            .args([ENGINE_FLAG, PARENT_FLAG, &std::process::id().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(stderr)
            .creation_flags(CREATE_NO_WINDOW.0)
            .spawn()
            .map_err(|e| EngineError::EngineProcess(format!("could not start: {e}")))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            kill(&mut child);
            return Err(EngineError::EngineProcess("no pipes to the process".into()));
        };

        let closing = Arc::new(AtomicBool::new(false));
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        let reader_closing = Arc::clone(&closing);
        let spawned = std::thread::Builder::new()
            .name("keyclean-engine-reader".into())
            .spawn(move || read_events(stdout, &event_tx, ready_tx, &reader_closing));
        if let Err(e) = spawned {
            kill(&mut child);
            return Err(EngineError::EngineProcess(format!(
                "could not start the event reader: {e}"
            )));
        }

        let ready = ready_rx.recv_timeout(READY_TIMEOUT).unwrap_or_else(|_| {
            Err(EngineError::EngineProcess(format!(
                "not ready within {} s",
                READY_TIMEOUT.as_secs()
            )))
        });
        if let Err(e) = ready {
            closing.store(true, Ordering::Release);
            kill(&mut child);
            return Err(e);
        }
        Ok((
            EngineClient {
                stdin: Mutex::new(Some(stdin)),
                child: Mutex::new(child),
                closing,
            },
            event_rx,
        ))
    }

    /// Asks the engine to lock the keyboard. The outcome arrives as events.
    pub fn lock(&self, request: LockRequest) -> Result<(), EngineError> {
        self.send(WireCommand::lock(request))
    }

    /// Asks the engine to end the current session (`EndReason::UserRequest`).
    pub fn unlock(&self) -> Result<(), EngineError> {
        self.send(WireCommand::Unlock)
    }

    /// Lists connected keyboards (name and id only). Runs in the app process: enumeration
    /// installs no hook.
    pub fn devices(&self) -> Result<Vec<KeyboardDevice>, EngineError> {
        devices::keyboards()
    }

    fn send(&self, command: WireCommand) -> Result<(), EngineError> {
        let mut guard = self.stdin.lock().unwrap_or_else(PoisonError::into_inner);
        let stdin = guard.as_mut().ok_or(EngineError::NotRunning)?;
        let mut line = to_line(&command);
        line.push('\n');
        stdin
            .write_all(line.as_bytes())
            .and_then(|()| stdin.flush())
            .map_err(|e| EngineError::EngineProcess(format!("could not send a command: {e}")))
    }
}

impl Drop for EngineClient {
    fn drop(&mut self) {
        self.closing.store(true, Ordering::Release);
        let _ = self.send(WireCommand::Shutdown);
        // Closing stdin asks the same (EOF), in case the line didn't get through.
        drop(
            self.stdin
                .get_mut()
                .unwrap_or_else(PoisonError::into_inner)
                .take(),
        );
        // `try_wait` decides, not the end of stdout: another process may hold an inherited
        // handle to that pipe and keep it open.
        let child = self.child.get_mut().unwrap_or_else(PoisonError::into_inner);
        let deadline = Instant::now() + EXIT_WAIT;
        while matches!(child.try_wait(), Ok(None)) && Instant::now() < deadline {
            std::thread::sleep(EXIT_POLL);
        }
        if matches!(child.try_wait(), Ok(None)) {
            // Stuck: killing it removes its hook.
            kill(child);
        }
    }
}

fn kill(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Reads event lines until the engine process closes its stdout. The first `ready` (or a failure
/// before it) goes to `ready`; everything else goes to `events`. If the process ends without the
/// app asking, reports the error and, since its hook died with it, that input is released.
fn read_events(
    stdout: impl Read,
    events: &Sender<EngineEvent>,
    ready: SyncSender<Result<(), EngineError>>,
    closing: &AtomicBool,
) {
    let mut ready = Some(ready);
    let mut active = false;
    for line in BufReader::new(stdout).lines() {
        let Ok(line) = line else { break };
        // Lines that aren't protocol messages are skipped; only the engine writes here.
        let Ok(wire) = serde_json::from_str::<WireEvent>(&line) else {
            continue;
        };
        let Some(event) = wire.into_event() else {
            if let Some(tx) = ready.take() {
                let _ = tx.send(Ok(()));
            }
            continue;
        };
        if let EngineEvent::Error(e) = &event
            && let Some(tx) = ready.take()
        {
            // The engine couldn't start; the process exits next.
            let _ = tx.send(Err(e.clone()));
            return;
        }
        if let EngineEvent::Status(status) = &event {
            active = status.state != SessionState::Idle;
        }
        let _ = events.send(event);
    }

    let error = EngineError::EngineProcess("the engine process exited".into());
    if let Some(tx) = ready.take() {
        let _ = tx.send(Err(error));
        return;
    }
    if closing.load(Ordering::Acquire) {
        return;
    }
    let _ = events.send(EngineEvent::Error(error));
    if active {
        let _ = events.send(EngineEvent::SessionEnded {
            reason: EndReason::EngineError,
        });
    }
    let _ = events.send(EngineEvent::Status(EngineStatus {
        state: SessionState::Idle,
        dev_cap: safety_profile() == SafetyProfile::Dev,
        session_remaining: None,
        hard_deadline_remaining: None,
    }));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineNotice;

    struct Run {
        ready: Result<(), EngineError>,
        events: Vec<EngineEvent>,
    }

    fn read(input: &str, closing: bool) -> Run {
        let (event_tx, event_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::sync_channel(1);
        read_events(
            input.as_bytes(),
            &event_tx,
            ready_tx,
            &AtomicBool::new(closing),
        );
        drop(event_tx);
        Run {
            ready: ready_rx.recv().unwrap(),
            events: event_rx.into_iter().collect(),
        }
    }

    fn line(event: &EngineEvent) -> String {
        format!("{}\n", to_line(&WireEvent::from(event)))
    }

    fn status(state: SessionState) -> EngineEvent {
        EngineEvent::Status(EngineStatus {
            state,
            dev_cap: true,
            session_remaining: None,
            hard_deadline_remaining: None,
        })
    }

    #[test]
    fn events_before_and_after_ready_are_forwarded() {
        let notice = EngineEvent::Notice(EngineNotice::PowerNotificationUnavailable);
        let ended = EngineEvent::SessionEnded {
            reason: EndReason::Emergency,
        };
        let input = format!(
            "{}{{\"event\":\"ready\"}}\nnot json\n{}",
            line(&notice),
            line(&ended)
        );
        let run = read(&input, true);
        assert_eq!(run.ready, Ok(()));
        assert_eq!(run.events, vec![notice, ended]);
    }

    #[test]
    fn a_start_failure_is_returned_from_start() {
        let input = line(&EngineEvent::Error(EngineError::ThreadSpawn("x".into())));
        let run = read(&input, false);
        match run.ready {
            Err(e) => assert_eq!(e.message_key(), "error.engine_start"),
            Ok(()) => panic!("expected a start failure"),
        }
        assert!(run.events.is_empty());
    }

    #[test]
    fn exit_before_ready_fails_start() {
        let run = read("", false);
        assert!(matches!(run.ready, Err(EngineError::EngineProcess(_))));
    }

    #[test]
    fn unexpected_exit_mid_lock_ends_the_session() {
        let input = format!(
            "{{\"event\":\"ready\"}}\n{}",
            line(&status(SessionState::Locked))
        );
        let run = read(&input, false);
        assert_eq!(run.ready, Ok(()));
        assert_eq!(run.events.len(), 4);
        assert!(matches!(
            run.events[1],
            EngineEvent::Error(EngineError::EngineProcess(_))
        ));
        assert_eq!(
            run.events[2],
            EngineEvent::SessionEnded {
                reason: EndReason::EngineError
            }
        );
        assert!(matches!(
            run.events[3],
            EngineEvent::Status(EngineStatus {
                state: SessionState::Idle,
                ..
            })
        ));
    }

    #[test]
    fn unexpected_exit_while_idle_reports_only_the_error() {
        let run = read("{\"event\":\"ready\"}\n", false);
        assert_eq!(run.events.len(), 2);
        assert!(matches!(
            run.events[0],
            EngineEvent::Error(EngineError::EngineProcess(_))
        ));
    }

    #[test]
    fn requested_exit_is_not_an_error() {
        let input = format!(
            "{{\"event\":\"ready\"}}\n{}",
            line(&status(SessionState::Idle))
        );
        let run = read(&input, true);
        assert_eq!(run.events, vec![status(SessionState::Idle)]);
    }
}
