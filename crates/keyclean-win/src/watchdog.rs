//! The watchdog thread: the hard deadline's second, independent enforcement (invariant 6).
//!
//! When an armed deadline passes, the watchdog
//! 1. switches the hook to `Passthrough`, so every event passes immediately, even if the engine
//!    thread is busy;
//! 2. posts `WM_WATCHDOG_EXPIRED` so the engine thread unhooks and ends the session;
//! 3. if the engine hasn't disarmed it within [`ESCALATION_GRACE`], terminates the process with
//!    `TerminateProcess` (exit code [`WATCHDOG_EXIT_CODE`]). Windows removes every hook of a
//!    process that exits, so input is released no matter what state the engine is in.
//!
//! Not `std::process::abort()`: on Windows that is a fail-fast crash, and Windows Error Reporting
//! may hold the dying process for several seconds while its hook stays installed (harness S19
//! measured about 5 s) [assumption: WER is the cause]. `TerminateProcess` on the current process
//! ends it without crash reporting; `abort()` remains the fallback if it fails.
//!
//! Deadlines are `Instant`s (QPC-backed). The thread sleeps on a condition variable; there is no
//! polling.

use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::hook;
use crate::msg::WM_WATCHDOG_EXPIRED;

/// How long the engine gets to release input after the watchdog fires before the process ends.
pub(crate) const ESCALATION_GRACE: Duration = Duration::from_secs(1);

/// Exit code of a process the watchdog ended ("KC").
pub const WATCHDOG_EXIT_CODE: u32 = 0x4B43;

#[derive(Default)]
struct State {
    deadline: Option<Instant>,
    generation: u64,
    shutdown: bool,
}

type Shared = Arc<(Mutex<State>, Condvar)>;

fn lock(shared: &Shared) -> MutexGuard<'_, State> {
    shared.0.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Handle to the watchdog thread. Owned by the engine thread.
pub(crate) struct Watchdog {
    shared: Shared,
    thread: Option<JoinHandle<()>>,
}

impl Watchdog {
    /// Starts the watchdog thread, idle.
    pub(crate) fn spawn() -> std::io::Result<Self> {
        let shared: Shared = Arc::new((Mutex::new(State::default()), Condvar::new()));
        let thread_shared = Arc::clone(&shared);
        let thread = std::thread::Builder::new()
            .name("keyclean-watchdog".into())
            .spawn(move || run(&thread_shared))?;
        Ok(Watchdog {
            shared,
            thread: Some(thread),
        })
    }

    /// Arms the watchdog for `deadline`, replacing any previous deadline. Returns the generation
    /// that its `WM_WATCHDOG_EXPIRED` will carry in `wParam`.
    pub(crate) fn arm(&self, deadline: Instant) -> u64 {
        let mut state = lock(&self.shared);
        state.deadline = Some(deadline);
        state.generation = state.generation.wrapping_add(1);
        self.shared.1.notify_all();
        state.generation
    }

    /// Disarms the watchdog. Called once input is released.
    pub(crate) fn disarm(&self) {
        let mut state = lock(&self.shared);
        state.deadline = None;
        state.generation = state.generation.wrapping_add(1);
        self.shared.1.notify_all();
    }
}

impl Drop for Watchdog {
    fn drop(&mut self) {
        {
            let mut state = lock(&self.shared);
            state.shutdown = true;
            state.deadline = None;
            self.shared.1.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn run(shared: &Shared) {
    loop {
        // Wait until the armed deadline passes (or shutdown).
        // Decided and acted on under the mutex: a disarm (and a following arm for a new session)
        // can't slip in between, so a stale expiry never switches a new session to passthrough.
        let fired_generation = {
            let mut state = lock(shared);
            loop {
                if state.shutdown {
                    return;
                }
                match state.deadline {
                    None => {
                        state = shared.1.wait(state).unwrap_or_else(PoisonError::into_inner);
                    }
                    Some(deadline) => {
                        let now = Instant::now();
                        if now >= deadline {
                            hook::force_passthrough();
                            hook::post(WM_WATCHDOG_EXPIRED, state.generation);
                            break state.generation;
                        }
                        state = shared
                            .1
                            .wait_timeout(state, deadline - now)
                            .unwrap_or_else(PoisonError::into_inner)
                            .0;
                    }
                }
            }
        };

        // Give the engine a moment to unhook and disarm; otherwise end the process.
        let give_up_at = Instant::now() + ESCALATION_GRACE;
        let mut state = lock(shared);
        loop {
            if state.shutdown || state.generation != fired_generation {
                break;
            }
            let now = Instant::now();
            if now >= give_up_at {
                // The engine thread is unresponsive while a hook may still be installed. Process
                // exit is the one release mechanism that cannot fail (Windows frees the hooks).
                terminate_self();
            }
            state = shared
                .1
                .wait_timeout(state, give_up_at - now)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }
}

/// Ends this process at once, without crash reporting. Windows then removes its hooks.
fn terminate_self() -> ! {
    // SAFETY: GetCurrentProcess returns a pseudo-handle for this process that needs no closing;
    // TerminateProcess on it ends every thread of the process, including this one.
    let _ = unsafe {
        windows::Win32::System::Threading::TerminateProcess(
            windows::Win32::System::Threading::GetCurrentProcess(),
            WATCHDOG_EXIT_CODE,
        )
    };
    // Only reached if TerminateProcess failed or hasn't taken effect yet.
    std::process::abort()
}
