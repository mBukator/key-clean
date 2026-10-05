//! Low-level keyboard and mouse hooks that record which tagged events got past KeyClean.
//!
//! Windows calls hooks newest-first, and a hook that blocks an event stops it from reaching the
//! older ones (LowLevelKeyboardProc / LowLevelMouseProc docs). The observer is installed before
//! the engine locks, so the engine's hooks are newer and run first: whatever the observer sees
//! passed KeyClean.
//!
//! The keyboard callback only increments atomic counters for events tagged with [`TAG`]; untagged
//! events (a person typing) are passed on without being looked at. It always passes events on.
//!
//! The mouse callback counts events tagged with [`MOUSE_TAG`] by kind. It swallows tagged button
//! presses, releases and wheel turns after counting them, so a probe that leaks past a broken lock
//! can't click or scroll whatever is under the cursor; tagged moves pass. Untagged mouse events (a
//! person's mouse or touchpad) are always passed on, and counted by kind only while the
//! `--mouse-diag` measurement turns counting on. The cursor position is never read.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc;
use std::thread::JoinHandle;

use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GetMessageW, HC_ACTION, KBDLLHOOKSTRUCT, LLKHF_UP, MSG, MSLLHOOKSTRUCT,
    PostThreadMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
    WM_MOUSEWHEEL, WM_QUIT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_XBUTTONDOWN, WM_XBUTTONUP,
};

use super::inject::TAG;
use super::mouse::MOUSE_TAG;

static DOWNS: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];
static UPS: [AtomicU32; 256] = [const { AtomicU32::new(0) }; 256];

/// Kinds of mouse event the observer counts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseKind {
    /// Pointer movement.
    Move,
    /// Left button pressed.
    LeftDown,
    /// Left button released.
    LeftUp,
    /// Right button pressed.
    RightDown,
    /// Right button released.
    RightUp,
    /// Middle button pressed.
    MiddleDown,
    /// Middle button released.
    MiddleUp,
    /// First side button pressed.
    X1Down,
    /// First side button released.
    X1Up,
    /// Second side button pressed.
    X2Down,
    /// Second side button released.
    X2Up,
    /// Vertical wheel turn.
    Wheel,
    /// Horizontal wheel turn.
    HWheel,
    /// Any other mouse message.
    Other,
}

impl MouseKind {
    /// Every kind, in counter order.
    pub const ALL: [MouseKind; 14] = [
        MouseKind::Move,
        MouseKind::LeftDown,
        MouseKind::LeftUp,
        MouseKind::RightDown,
        MouseKind::RightUp,
        MouseKind::MiddleDown,
        MouseKind::MiddleUp,
        MouseKind::X1Down,
        MouseKind::X1Up,
        MouseKind::X2Down,
        MouseKind::X2Up,
        MouseKind::Wheel,
        MouseKind::HWheel,
        MouseKind::Other,
    ];

    /// A short name for reports.
    pub const fn name(self) -> &'static str {
        match self {
            MouseKind::Move => "move",
            MouseKind::LeftDown => "left down",
            MouseKind::LeftUp => "left up",
            MouseKind::RightDown => "right down",
            MouseKind::RightUp => "right up",
            MouseKind::MiddleDown => "middle down",
            MouseKind::MiddleUp => "middle up",
            MouseKind::X1Down => "X1 down",
            MouseKind::X1Up => "X1 up",
            MouseKind::X2Down => "X2 down",
            MouseKind::X2Up => "X2 up",
            MouseKind::Wheel => "wheel",
            MouseKind::HWheel => "horizontal wheel",
            MouseKind::Other => "other",
        }
    }

    /// Classifies a low-level mouse message. For the side buttons the hook receives the button
    /// number in the high word of `mouseData` (1 = XBUTTON1, 2 = XBUTTON2).
    fn classify(message: u32, mouse_data: u32) -> MouseKind {
        match message {
            WM_MOUSEMOVE => MouseKind::Move,
            WM_LBUTTONDOWN => MouseKind::LeftDown,
            WM_LBUTTONUP => MouseKind::LeftUp,
            WM_RBUTTONDOWN => MouseKind::RightDown,
            WM_RBUTTONUP => MouseKind::RightUp,
            WM_MBUTTONDOWN => MouseKind::MiddleDown,
            WM_MBUTTONUP => MouseKind::MiddleUp,
            WM_XBUTTONDOWN => match mouse_data >> 16 {
                1 => MouseKind::X1Down,
                2 => MouseKind::X2Down,
                _ => MouseKind::Other,
            },
            WM_XBUTTONUP => match mouse_data >> 16 {
                1 => MouseKind::X1Up,
                2 => MouseKind::X2Up,
                _ => MouseKind::Other,
            },
            WM_MOUSEWHEEL => MouseKind::Wheel,
            WM_MOUSEHWHEEL => MouseKind::HWheel,
            _ => MouseKind::Other,
        }
    }
}

const KINDS: usize = MouseKind::ALL.len();

static MOUSE_TAGGED: [AtomicU32; KINDS] = [const { AtomicU32::new(0) }; KINDS];
static MOUSE_UNTAGGED: [AtomicU32; KINDS] = [const { AtomicU32::new(0) }; KINDS];
/// Whether untagged mouse events are counted (only during the `--mouse-diag` measurement).
static COUNT_UNTAGGED: AtomicBool = AtomicBool::new(false);

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

unsafe extern "system" fn mouse_observer_proc(
    code: i32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        // SAFETY: for WH_MOUSE_LL with HC_ACTION, `lparam` points to an MSLLHOOKSTRUCT valid for
        // the duration of this call. Only `mouseData` and `dwExtraInfo` are read, never `pt`.
        let (mouse_data, extra) = unsafe {
            let info = &*(lparam.0 as *const MSLLHOOKSTRUCT);
            (info.mouseData, info.dwExtraInfo)
        };
        let kind = MouseKind::classify(wparam.0 as u32, mouse_data);
        if extra == MOUSE_TAG {
            MOUSE_TAGGED[kind as usize].fetch_add(1, Ordering::Relaxed);
            if kind != MouseKind::Move {
                // A tagged click or wheel turn that got this far must not reach any window.
                return LRESULT(1);
            }
        } else if COUNT_UNTAGGED.load(Ordering::Relaxed) {
            MOUSE_UNTAGGED[kind as usize].fetch_add(1, Ordering::Relaxed);
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

/// The running observer hooks. Stop when dropped. Only one may exist at a time.
pub struct Observer {
    thread_id: u32,
    thread: Option<JoinHandle<()>>,
}

impl Observer {
    /// Installs the keyboard and mouse observer hooks on their own thread.
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
                // SAFETY: as above, for `mouse_observer_proc` and WH_MOUSE_LL.
                let mouse_hook = match unsafe {
                    SetWindowsHookExW(
                        WH_MOUSE_LL,
                        Some(mouse_observer_proc),
                        Some(module.into()),
                        0,
                    )
                } {
                    Ok(h) => h,
                    Err(e) => {
                        // SAFETY: `hook` was installed by this thread and is removed once.
                        let _ = unsafe { UnhookWindowsHookEx(hook) };
                        let _ = ready_tx.send(Err(format!("mouse observer hook: {e}")));
                        return;
                    }
                };
                // SAFETY: no preconditions.
                let _ = ready_tx.send(Ok(unsafe { GetCurrentThreadId() }));
                let mut msg = MSG::default();
                // SAFETY: valid MSG buffer; runs until WM_QUIT (0) or an error (-1).
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {}
                // SAFETY: both hooks were installed by this thread and are removed once.
                let _ = unsafe { UnhookWindowsHookEx(mouse_hook) };
                // SAFETY: as above.
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

    /// Clears all tagged counters (keys and mouse).
    pub fn reset(&self) {
        for i in 0..256 {
            DOWNS[i].store(0, Ordering::Relaxed);
            UPS[i].store(0, Ordering::Relaxed);
        }
        for counter in &MOUSE_TAGGED {
            counter.store(0, Ordering::Relaxed);
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

    /// All tagged key events that got past KeyClean since the last reset.
    pub fn total(&self) -> u32 {
        (0..256)
            .map(|i| DOWNS[i].load(Ordering::Relaxed) + UPS[i].load(Ordering::Relaxed))
            .sum()
    }

    /// Tagged mouse events of `kind` that got past KeyClean since the last reset.
    pub fn mouse(&self, kind: MouseKind) -> u32 {
        MOUSE_TAGGED[kind as usize].load(Ordering::Relaxed)
    }

    /// Untagged mouse events of `kind` (a person's mouse or touchpad) that got past KeyClean while
    /// counting was on (see [`count_untagged_mouse`](Self::count_untagged_mouse)). Counts only.
    pub fn untagged_mouse(&self, kind: MouseKind) -> u32 {
        MOUSE_UNTAGGED[kind as usize].load(Ordering::Relaxed)
    }

    /// Starts (clearing the counters first) or stops counting untagged mouse events. Off by
    /// default; only the `--mouse-diag` measurement turns it on.
    pub fn count_untagged_mouse(&self, on: bool) {
        if on {
            for counter in &MOUSE_UNTAGGED {
                counter.store(0, Ordering::Relaxed);
            }
        }
        COUNT_UNTAGGED.store(on, Ordering::Release);
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
