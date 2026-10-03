//! A low-level keyboard hook that records which tagged events got past KeyClean.
//!
//! Windows calls hooks newest-first, and a hook that blocks an event stops it from reaching the
//! older ones (LowLevelKeyboardProc docs). The observer is installed before the engine locks, so
//! the engine's hook is newer and runs first: whatever the observer sees passed KeyClean.
//!
//! The callback only increments atomic counters for events tagged with [`TAG`]; untagged events (a
//! person typing) are passed on without being looked at. It always passes events on.

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_UP, MSG, PostThreadMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WM_QUIT,
};

use super::inject::TAG;

static DOWNS: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];
static UPS: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];

unsafe extern "system" fn observer_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        // SAFETY: for WH_KEYBOARD_LL with HC_ACTION, `lparam` points to a KBDLLHOOKSTRUCT valid for
        // the duration of this call.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        if info.dwExtraInfo == TAG {
            let slot = (info.vkCode & 0xFF) as usize;
            let counters = if info.flags.0 & LLKHF_UP.0 != 0 {
                &UPS
            } else {
                &DOWNS
            };
            counters[slot].fetch_add(1, Ordering::Relaxed);
        }
    }
    // SAFETY: forwarding the unmodified arguments to the next hook.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

/// Error starting the observer.
#[derive(Debug)]
pub struct ObserverError(pub String);

impl std::fmt::Display for ObserverError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ObserverError {}

/// The running observer hook. Stops when dropped. Only one may exist at a time.
pub struct Observer {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Observer {
    /// Installs the observer hook on its own thread.
    pub fn start() -> Result<Observer, ObserverError> {
        let (ready_tx, ready_rx) = mpsc::sync_channel::<Result<u32, String>>(1);
        let thread = std::thread::Builder::new()
            .name("keyclean-e2e-observer".into())
            .spawn(move || {
                // SAFETY: returns this module's handle without transferring ownership.
                let module = match unsafe { GetModuleHandleW(None) } {
                    Ok(m) => m,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                // SAFETY: `observer_proc` matches HOOKPROC and lives for the whole program; this
                // thread runs a message loop below.
                let hook = match unsafe {
                    SetWindowsHookExW(WH_KEYBOARD_LL, Some(observer_proc), Some(module.into()), 0)
                } {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = ready_tx.send(Err(e.to_string()));
                        return;
                    }
                };
                // SAFETY: no preconditions.
                let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));
                let mut msg = MSG::default();
                // SAFETY: valid MSG buffer; runs until WM_QUIT (0) or an error (-1).
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {}
                // SAFETY: `hook` was installed by this thread and is removed once.
                let _ = unsafe { UnhookWindowsHookEx(hook) };
            })
            .map_err(|e| ObserverError(e.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(thread_id)) => Ok(Observer {
                thread_id,
                thread: Some(thread),
            }),
            Ok(Err(e)) => Err(ObserverError(e)),
            Err(_) => Err(ObserverError("observer thread exited".into())),
        }
    }

    /// Clears all counters.
    pub fn reset(&self) {
        for i in 0..256 {
            DOWNS[i].store(0, Ordering::Relaxed);
            UPS[i].store(0, Ordering::Relaxed);
        }
    }

    /// Tagged key-downs of `vk` that got past KeyClean since the last reset.
    pub fn downs(&self, vk: u8) -> u32 {
        DOWNS[usize::from(vk)].load(Ordering::Relaxed)
    }

    /// Tagged key-ups of `vk` that got past KeyClean since the last reset.
    pub fn ups(&self, vk: u8) -> u32 {
        UPS[usize::from(vk)].load(Ordering::Relaxed)
    }

    /// All tagged events that got past KeyClean since the last reset.
    pub fn total(&self) -> u32 {
        (0..256)
            .map(|i| DOWNS[i].load(Ordering::Relaxed) + UPS[i].load(Ordering::Relaxed))
            .sum()
    }
}

impl Drop for Observer {
    fn drop(&mut self) {
        // SAFETY: posting WM_QUIT to the observer thread's queue; fails harmlessly if it's gone.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
