//! Process-window and workstation helpers for the harness.

use windows::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, GetLastError, HWND, LPARAM, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::Shutdown::LockWorkStation;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::{GetRegisteredRawInputDevices, RAWINPUTDEVICE};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GWL_EXSTYLE, GetClassNameW, GetCursorPos, GetForegroundWindow, GetWindowLongPtrW,
    GetWindowRect, GetWindowThreadProcessId, IsWindow, IsWindowVisible, PostMessageW, SC_CLOSE,
    SHOW_WINDOW_CMD, SMTO_ABORTIFHUNG, SW_HIDE, SW_MINIMIZE, SendMessageTimeoutW, ShowWindowAsync,
    WM_NULL, WM_SYSCOMMAND, WS_EX_TOPMOST,
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
        request_close_window(hwnd);
    }
    targets.len()
}

/// Asks one window to close, exactly as clicking its X button does (`WM_SYSCOMMAND` /
/// `SC_CLOSE`). Returns false if the request couldn't be posted (e.g. the window is gone).
pub fn request_close_window(hwnd: isize) -> bool {
    // SAFETY: PostMessageW only enqueues; a window that just closed makes it fail harmlessly.
    unsafe {
        PostMessageW(
            Some(HWND(hwnd as *mut _)),
            WM_SYSCOMMAND,
            WPARAM(SC_CLOSE as usize),
            LPARAM(0),
        )
    }
    .is_ok()
}

/// Whether the window has the always-on-top extended style (`WS_EX_TOPMOST`).
fn is_topmost(hwnd: isize) -> bool {
    // SAFETY: reading a window's extended style has no preconditions; a window that just closed
    // makes it return 0.
    let ex_style = unsafe { GetWindowLongPtrW(HWND(hwnd as *mut _), GWL_EXSTYLE) };
    (ex_style as u32) & WS_EX_TOPMOST.0 != 0
}

/// The app's full-screen overlay: the first visible, always-on-top (`WS_EX_TOPMOST`) top-level
/// window of process `pid` with class `class`, with its rectangle. The app's main window isn't
/// topmost, so during a lock this is the overlay.
pub fn overlay_window(pid: u32, class: &str) -> Option<(isize, Rect)> {
    visible_windows(pid)
        .into_iter()
        .filter(|&hwnd| is_topmost(hwnd) && window_info(hwnd).class == class)
        .find_map(|hwnd| Some((hwnd, rect_of(hwnd)?)))
}

/// The first visible top-level window of process `pid` with class `class` that isn't topmost:
/// the app's main window, not its overlay.
pub fn main_window(pid: u32, class: &str) -> Option<isize> {
    visible_windows(pid)
        .into_iter()
        .find(|&hwnd| !is_topmost(hwnd) && window_info(hwnd).class == class)
}

/// Whether `hwnd` still names a window (visible or not). A destroyed window doesn't.
pub fn window_exists(hwnd: isize) -> bool {
    // SAFETY: IsWindow accepts any value, including handles of destroyed windows.
    unsafe { IsWindow(Some(HWND(hwnd as *mut _))) }.as_bool()
}

fn show_async(hwnd: isize, cmd: SHOW_WINDOW_CMD) -> bool {
    // SAFETY: ShowWindowAsync only posts the request to the window's thread, so a window of
    // another process can't make this call wait; a window that just closed makes it fail
    // harmlessly.
    unsafe { ShowWindowAsync(HWND(hwnd as *mut _), cmd) }.as_bool()
}

/// Hides a window (`SW_HIDE`), as another program could. Returns false if the request couldn't
/// be posted.
pub fn hide_window(hwnd: isize) -> bool {
    show_async(hwnd, SW_HIDE)
}

/// Minimizes a window (`SW_MINIMIZE`), as another program could. Returns false if the request
/// couldn't be posted.
pub fn minimize_window(hwnd: isize) -> bool {
    show_async(hwnd, SW_MINIMIZE)
}

/// Makes this process per-monitor DPI aware, so window positions and mouse coordinates are both
/// in physical pixels. Returns false if Windows refused (e.g. awareness was already set).
pub fn make_dpi_aware() -> bool {
    // SAFETY: no preconditions; fails harmlessly if the awareness is already set.
    unsafe { SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) }.is_ok()
}

/// Screen rectangle of a window, in physical pixels (after [`make_dpi_aware`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    /// Left edge.
    pub left: i32,
    /// Top edge.
    pub top: i32,
    /// Right edge.
    pub right: i32,
    /// Bottom edge.
    pub bottom: i32,
}

impl From<RECT> for Rect {
    fn from(rect: RECT) -> Self {
        Rect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        }
    }
}

fn rect_of(hwnd: isize) -> Option<Rect> {
    let mut rect = RECT::default();
    // SAFETY: `rect` is writable; a window that just closed makes the call fail harmlessly.
    unsafe { GetWindowRect(HWND(hwnd as *mut _), &mut rect) }.ok()?;
    Some(rect.into())
}

/// The rectangle of the first visible top-level window of process `pid` with class `class`.
pub fn window_rect(pid: u32, class: &str) -> Option<Rect> {
    let hwnd = visible_windows(pid)
        .into_iter()
        .find(|&hwnd| window_info(hwnd).class == class)?;
    rect_of(hwnd)
}

/// The full rectangle (`rcMonitor`, not the work area) of the monitor under the cursor, in
/// physical pixels (after [`make_dpi_aware`]).
pub fn monitor_rect_under_cursor() -> Option<Rect> {
    let mut point = POINT::default();
    // SAFETY: `point` is writable.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    // SAFETY: no preconditions; MONITOR_DEFAULTTONEAREST always returns a monitor.
    let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
    let mut info = MONITORINFO {
        cbSize: std::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info` is writable and its `cbSize` names the MONITORINFO layout.
    unsafe { GetMonitorInfoW(monitor, &mut info) }
        .as_bool()
        .then(|| info.rcMonitor.into())
}

/// The foreground window, if there is one.
pub fn foreground_hwnd() -> Option<isize> {
    // SAFETY: no preconditions; returns a null handle if no window is in the foreground.
    let hwnd = unsafe { GetForegroundWindow() };
    (!hwnd.is_invalid()).then_some(hwnd.0 as isize)
}

/// The process that owns the foreground window, if there is one.
pub fn foreground_pid() -> Option<u32> {
    // SAFETY: no preconditions; returns a null handle if no window is in the foreground.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is writable; a window that just closed makes the call return 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    (pid != 0).then_some(pid)
}

/// The foreground window's class name and owning process id, for diagnostics.
pub fn foreground_window() -> Option<(String, u32)> {
    // SAFETY: no preconditions; returns a null handle if no window is in the foreground.
    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is writable; a window that just closed makes the call return 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    Some((window_info(hwnd.0 as isize).class, pid))
}

/// Locks the workstation, as Win+L does. The user has to sign back in.
pub fn lock_workstation() -> windows::core::Result<()> {
    // SAFETY: no preconditions; returns an error if the workstation can't be locked.
    unsafe { LockWorkStation() }
}

/// How many Raw Input device classes the calling process has registered. Zero while the engine is
/// idle (invariant 4). Returns `None` if Windows reports an error.
pub fn registered_raw_input_count() -> Option<usize> {
    let mut count = 0u32;
    // SAFETY: a NULL buffer asks only for the number of registered devices, written to `count`.
    let result = unsafe {
        GetRegisteredRawInputDevices(
            None,
            &mut count,
            std::mem::size_of::<RAWINPUTDEVICE>() as u32,
        )
    };
    // SAFETY: GetLastError has no preconditions.
    if result == u32::MAX && unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER {
        return None;
    }
    Some(count as usize)
}
