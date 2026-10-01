//! Process-window and workstation helpers for the harness.

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, WM_CLOSE,
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

/// Posts `WM_CLOSE` to every visible top-level window of process `pid` (like clicking X).
/// Returns how many windows were asked to close.
pub fn close_windows_of(pid: u32) -> usize {
    let windows = visible_windows(pid);
    for &hwnd in &windows {
        // SAFETY: PostMessageW only enqueues; a window that just closed makes it fail harmlessly.
        let _ = unsafe { PostMessageW(Some(HWND(hwnd as *mut _)), WM_CLOSE, WPARAM(0), LPARAM(0)) };
    }
    windows.len()
}

/// Locks the workstation, as Win+L does. The user has to sign back in.
pub fn lock_workstation() -> windows::core::Result<()> {
    // SAFETY: no preconditions; returns an error if the workstation can't be locked.
    unsafe { LockWorkStation() }
}
