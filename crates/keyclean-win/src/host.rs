//! Engine-host mode (ADR 0009): `keyclean.exe --engine --parent <pid>` runs the engine in a
//! process of its own, with no WebView.
//!
//! Windows intermittently stops calling a low-level keyboard hook while a WebView2 window of the
//! hook's own process has focus. In this process there is no WebView, so the app's window can't
//! affect the hook.
//!
//! The app sends commands on stdin and receives session events on stdout, one JSON object per
//! line (see `protocol`). Never key data. Ways this process ends:
//! - stdin reaches EOF (the app closed it or died) or a `shutdown` command arrives: the engine is
//!   dropped, which releases any lock, then the process exits;
//! - the app process ends (watched separately, in case stdin stays open): the process exits;
//! - a write to stdout fails (the app is gone): the process exits;
//! - the engine thread stops on its own: its last events say why, then the process exits.
//!
//! Whenever the process exits, Windows removes its hook, so input is released (invariant 1).

use std::io::{self, BufRead, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    INFINITE, OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
};

use crate::engine::{Engine, EngineEvent, LockRequest};
use crate::protocol::{WireCommand, WireEvent, to_line};

/// First argument that selects engine-host mode.
pub const ENGINE_FLAG: &str = "--engine";
/// Followed by the app's process id.
pub const PARENT_FLAG: &str = "--parent";

/// Exit codes of the engine process.
const EXIT_OK: i32 = 0;
const EXIT_START_FAILED: i32 = 1;
const EXIT_BAD_ARGS: i32 = 2;
const EXIT_OUTPUT_FAILED: i32 = 3;
const EXIT_ENGINE_STOPPED: i32 = 4;
const EXIT_PARENT_GONE: i32 = 5;

/// How long to wait for the last events to be written after the engine stopped.
const FLUSH_WAIT: Duration = Duration::from_secs(1);

/// Set before the engine is dropped on purpose, so the event forwarder doesn't treat the end of
/// the event stream as a failure.
static STOPPING: AtomicBool = AtomicBool::new(false);

/// Parses the arguments after [`ENGINE_FLAG`]: `--parent <pid>`.
pub fn parse_parent(args: &[String]) -> Option<u32> {
    match args {
        [flag, pid] if flag == PARENT_FLAG => pid.parse().ok(),
        _ => None,
    }
}

/// Runs engine-host mode with the arguments after [`ENGINE_FLAG`]. Returns the exit code.
pub fn run_from_args(args: &[String]) -> i32 {
    match parse_parent(args) {
        Some(parent_pid) => run(parent_pid),
        None => {
            note("usage: keyclean --engine --parent <pid>");
            EXIT_BAD_ARGS
        }
    }
}

/// Runs the engine until the app shuts it down or goes away. Returns the exit code.
pub fn run(parent_pid: u32) -> i32 {
    watch_parent(parent_pid);

    let (engine, events) = match Engine::start() {
        Ok(started) => started,
        Err(e) => {
            let _ = write_line(&WireEvent::from(&EngineEvent::Error(e)));
            return EXIT_START_FAILED;
        }
    };
    // Written before the forwarder starts, so `ready` always comes first and stdout has a single
    // writer at a time.
    if write_line(&WireEvent::Ready).is_err() {
        return EXIT_OUTPUT_FAILED;
    }
    let forwarded = match spawn_forwarder(events) {
        Ok(done) => done,
        Err(e) => {
            note(&format!("could not start the event forwarder: {e}"));
            return EXIT_START_FAILED;
        }
    };

    // This thread only reads: it never writes to stdout, so a full pipe can't stop it from seeing
    // `shutdown` or EOF.
    for line in io::stdin().lock().lines() {
        let Ok(line) = line else { break };
        let command = match serde_json::from_str::<WireCommand>(&line) {
            Ok(command) => command,
            Err(e) => {
                note(&format!("ignored an unreadable command: {e}"));
                continue;
            }
        };
        let sent = match command {
            WireCommand::Shutdown => break,
            WireCommand::Unlock => engine.unlock(),
            WireCommand::Lock { .. } => {
                Option::<LockRequest>::from(command).map_or(Ok(()), |r| engine.lock(r))
            }
        };
        if let Err(e) = sent {
            // The engine thread stopped; the forwarder reports it and ends the process.
            note(&format!("command not delivered: {}", e.details()));
        }
    }

    STOPPING.store(true, Ordering::Release);
    drop(engine); // Releases any lock and stops the engine thread.
    let _ = forwarded.recv_timeout(FLUSH_WAIT);
    EXIT_OK
}

/// Writes engine events to stdout until the engine stops. Ends the process if the app can't be
/// reached or the engine stopped on its own.
fn spawn_forwarder(events: Receiver<EngineEvent>) -> io::Result<Receiver<()>> {
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("keyclean-host-events".into())
        .spawn(move || {
            for event in events {
                if write_line(&WireEvent::from(&event)).is_err() {
                    // The app is gone. Exiting removes the hook.
                    std::process::exit(EXIT_OUTPUT_FAILED);
                }
            }
            if !STOPPING.load(Ordering::Acquire) {
                // The engine thread stopped by itself; the events above say why.
                std::process::exit(EXIT_ENGINE_STOPPED);
            }
            let _ = done_tx.send(());
        })?;
    Ok(done_rx)
}

/// Ends this process when the app process ends, even if stdin stays open (another process may
/// hold an inherited handle to the pipe). If the app can't be opened, stdin EOF is relied on.
fn watch_parent(parent_pid: u32) {
    let spawned = std::thread::Builder::new()
        .name("keyclean-parent-watch".into())
        .spawn(move || {
            // SAFETY: OpenProcess has no memory-safety preconditions; the handle is closed below.
            let handle = match unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, parent_pid) } {
                Ok(handle) => handle,
                Err(e) => {
                    note(&format!("can't watch the app process: {e}"));
                    return;
                }
            };
            // SAFETY: `handle` is a valid process handle opened with SYNCHRONIZE access.
            let waited = unsafe { WaitForSingleObject(handle, INFINITE) };
            // SAFETY: `handle` came from OpenProcess and is closed exactly once.
            let _ = unsafe { CloseHandle(handle) };
            if waited == WAIT_OBJECT_0 {
                // The app is gone. Exiting removes the hook and releases input.
                std::process::exit(EXIT_PARENT_GONE);
            }
        });
    if let Err(e) = spawned {
        note(&format!("could not start the app watcher: {e}"));
    }
}

/// Writes one protocol line to stdout and flushes it.
fn write_line(event: &WireEvent) -> io::Result<()> {
    let mut out = io::stdout().lock();
    out.write_all(to_line(event).as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

/// A diagnostic on stderr. Never panics (unlike `eprintln!`), so a closed stderr can't abort the
/// engine. Never contains key data.
fn note(message: &str) {
    let _ = writeln!(io::stderr(), "[keyclean-engine] {message}");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_the_parent_pid() {
        assert_eq!(parse_parent(&args(&["--parent", "1234"])), Some(1234));
    }

    #[test]
    fn rejects_bad_arguments() {
        assert_eq!(parse_parent(&args(&[])), None);
        assert_eq!(parse_parent(&args(&["--parent"])), None);
        assert_eq!(parse_parent(&args(&["--parent", "abc"])), None);
        assert_eq!(parse_parent(&args(&["--parent", "-1"])), None);
        assert_eq!(parse_parent(&args(&["--other", "1234"])), None);
        assert_eq!(parse_parent(&args(&["--parent", "1", "extra"])), None);
    }
}
