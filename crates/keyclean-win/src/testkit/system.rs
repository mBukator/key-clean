//! Process-window and workstation helpers for the harness.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetClassNameW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SC_CLOSE,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW, WM_NULL, WM_SYSCOMMAND,
};
use windows::core::BOOL;

struct Search {
    pid: u32,
    found: Vec<isize>,
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the `&mut Search` passed to EnumWindows below, alive for the whole
    // enumeration, which runs synchronously on this thread.
    let search = unsafe { &mut *(lparam.0 as *mut Search) };
    let mut pid = 0u32;
    // SAFETY: `hwnd` comes from EnumWindows; `pid` is writable.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    // SAFETY: as above.
    if pid == search.pid && unsafe { IsWindowVisible(hwnd) }.as_bool() {
        search.found.push(hwnd.0 as isize);
    }
    BOOL(1)
}

fn visible_windows(pid: u32) -> Vec<isize> {
    let mut search = Search {
        pid,
        found: Vec::new(),
    };
    // SAFETY: `collect` matches WNDENUMPROC and only uses `lparam` as the `Search` it points to,
    // which outlives this call.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&mut search as *mut Search as isize)) };
    search.found
}

/// Number of visible top-level windows owned by process `pid`.
pub fn visible_window_count(pid: u32) -> usize {
    visible_windows(pid).len()
}

/// A visible top-level window of a process, for diagnostics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowInfo {
    /// The window class name (e.g. tao's window class), not its contents.
    pub class: String,
    /// Whether the window's thread answered a no-op message within 1 s.
    pub responding: bool,
}

fn window_info(hwnd: isize) -> WindowInfo {
    let hwnd = HWND(hwnd as *mut _);
    let mut buffer = [0u16; 256];
    // SAFETY: `buffer` is writable for its whole length, which the wrapper passes as the maximum.
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let class = String::from_utf16_lossy(&buffer[..usize::try_from(len).unwrap_or(0)]);
    // SAFETY: WM_NULL does nothing; SMTO_ABORTIFHUNG returns at once if the thread is hung, and
    // the 1 s timeout bounds the wait otherwise. A zero result means timeout or failure.
    let answered = unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_NULL,
            WPARAM(0),
            LPARAM(0),
            SMTO_ABORTIFHUNG,
            1000,
            None,
        )
    };
    WindowInfo {
        class,
        responding: answered.0 != 0,
    }
}

/// Class name and responsiveness of every visible top-level window of process `pid`.
pub fn windows_of(pid: u32) -> Vec<WindowInfo> {
    visible_windows(pid).into_iter().map(window_info).collect()
}

/// Asks the visible top-level windows of process `pid` whose class is `class` to close, exactly as
/// clicking their X button does (`WM_SYSCOMMAND` / `SC_CLOSE`). Returns how many were asked.
///
/// Only the app's own windows must be targeted: closing a framework's hidden helper windows (for
/// example tao's "Tao Thread Event Target") breaks the app's event loop, which no user action does.
pub fn request_close(pid: u32, class: &str) -> usize {
    let targets: Vec<isize> = visible_windows(pid)
        .into_iter()
        .filter(|&hwnd| window_info(hwnd).class == class)
        .collect();
    for &hwnd in &targets {
        // SAFETY: PostMessageW only enqueues; a window that just closed makes it fail harmlessly.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                WM_SYSCOMMAND,
                WPARAM(SC_CLOSE as usize),
                LPARAM(0),
            )
        };
    }
    targets.len()
}

/// Locks the workstation, as Win+L does. The user has to sign back in.
pub fn lock_workstation() -> windows::core::Result<()> {
    // SAFETY: no preconditions; returns an error if the workstation can't be locked.
    unsafe { LockWorkStation() }
}
