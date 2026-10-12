//! Turns `WM_CLOSE` on the app's hidden helper windows into a normal app exit.
//!
//! `taskkill` without `/F` asks a process to close by sending `WM_CLOSE` to its top-level windows
//! [assumption, tested 2026-10-02: harness S11 first version]. The webview window handles it (Tauri
//! raises `CloseRequested`), but tao's thread event target window and the single-instance plugin's
//! window pass it to `DefWindowProcW`, which destroys them (tao 0.37.1 `event_loop.rs`,
//! tauri-plugin-single-instance 2.5.2 `platform_impl/windows.rs`). Without its event target window
//! tao's event loop stops working and the UI hangs. The engine runs in its own process and still
//! ends the lock, but the app should exit cleanly instead.
//!
//! [`guard_helper_windows`] subclasses those helper windows so `WM_CLOSE` calls the app's exit
//! callback and leaves the window alone. Subclassing only works from the thread that owns the window
//! (`SetWindowSubclass` docs), so it must run on the app's main thread after the windows exist (in
//! Tauri's `setup`).
//!
//! From M8, closing the dashboard sends KeyClean to the tray instead of exiting (ADR 0016). A
//! `WM_CLOSE` that `taskkill` sends to the dashboard must still exit. [`guard_dashboard_close`]
//! tells the two apart: the close button, Alt+F4 and the window menu send `WM_SYSCOMMAND` with
//! `SC_CLOSE`, which `DefWindowProc` turns into `WM_CLOSE` [docs: `WM_SYSCOMMAND`]; `taskkill`
//! sends `WM_CLOSE` alone [assumption, tested 2026-10-02 and in M6 harness S23]. A `WM_CLOSE` with
//! no `SC_CLOSE` just before it calls the exit callback.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GCLP_HMODULE, GetClassLongPtrW, GetClassNameW, SC_CLOSE, WM_CLOSE,
    WM_NCDESTROY, WM_SYSCOMMAND,
};
use windows::core::BOOL;

/// Class of Tauri's webview windows, which handle `WM_CLOSE` themselves.
const WEBVIEW_CLASS: &str = "Tauri Window";
/// Subclass id; any constant unique to this module.
const SUBCLASS_ID: usize = 0x4B43_434C;
/// Subclass id of the dashboard guard.
const DASHBOARD_SUBCLASS_ID: usize = 0x4B43_4442;

static ON_CLOSE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

/// Set by the dashboard's `SC_CLOSE` and taken by the `WM_CLOSE` it produces. One dashboard at a
/// time, all on the main thread.
static USER_CLOSE: AtomicBool = AtomicBool::new(false);

/// Subclasses every top-level window of the calling thread that this executable registered,
/// except the webview windows, so `WM_CLOSE` calls `on_close` instead of destroying it. Returns how
/// many windows were guarded.
///
/// Call it on the app's main thread once the helper windows exist. Calling it again guards
/// windows created since (already guarded ones are unaffected), but keeps the first callback.
pub fn guard_helper_windows(on_close: impl Fn() + Send + Sync + 'static) -> usize {
    let _ = ON_CLOSE.set(Box::new(on_close));
    let mut guarded = 0usize;
    // SAFETY: the callback only runs during this call, and `guarded` outlives it; LPARAM carries a
    // pointer to it.
    let _ = unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(guard_window),
            LPARAM(&mut guarded as *mut usize as isize),
        )
    };
    guarded
}

/// Subclasses this thread's webview windows (the dashboard) so a `WM_CLOSE` the user didn't
/// start with the close button, Alt+F4 or the window menu calls the exit callback given to
/// [`guard_helper_windows`] instead of closing the window. A user's close goes on as usual (the
/// dashboard to the tray). Returns how many windows are guarded, counting ones guarded earlier.
///
/// The windows are found here rather than passed in: asking Tauri for the dashboard's handle from
/// a task on the main thread reported it unavailable (M8 harness S44, 2026-10-12). An overlay
/// open at the time is guarded too, which changes nothing: it exits on any close request
/// (ADR 0014).
///
/// Call it on the app's main thread, which owns the webview windows, after
/// [`guard_helper_windows`].
pub fn guard_dashboard_close() -> usize {
    let mut guarded = 0usize;
    // SAFETY: the callback only runs during this call, and `guarded` outlives it; LPARAM carries a
    // pointer to it.
    let _ = unsafe {
        EnumThreadWindows(
            GetCurrentThreadId(),
            Some(guard_webview_window),
            LPARAM(&mut guarded as *mut usize as isize),
        )
    };
    guarded
}

/// `EnumThreadWindows` callback for [`guard_dashboard_close`].
///
/// # Safety
/// Called by Windows during [`guard_dashboard_close`], with `lparam` pointing to its counter.
unsafe extern "system" fn guard_webview_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if is_own_webview(hwnd) {
        // SAFETY: `hwnd` belongs to the calling thread (EnumThreadWindows), as SetWindowSubclass
        // requires; `dashboard_subclass` lives for the whole program. Setting it again on a
        // window that has it only replaces its (unused) data.
        if unsafe { SetWindowSubclass(hwnd, Some(dashboard_subclass), DASHBOARD_SUBCLASS_ID, 0) }
            .as_bool()
        {
            // SAFETY: `lparam` is the counter pointer passed by `guard_dashboard_close`, which
            // is still running.
            unsafe { *(lparam.0 as *mut usize) += 1 };
        }
    }
    TRUE
}

/// The dashboard's subclass procedure.
///
/// # Safety
/// Called by Windows only, with valid subclass-procedure arguments.
unsafe extern "system" fn dashboard_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        // The low four bits are used internally by Windows [docs: WM_SYSCOMMAND].
        WM_SYSCOMMAND if (wparam.0 & 0xFFF0) == SC_CLOSE as usize => {
            USER_CLOSE.store(true, Ordering::SeqCst);
            // SAFETY: default processing with the original arguments; it sends WM_CLOSE.
            let result = unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) };
            USER_CLOSE.store(false, Ordering::SeqCst);
            result
        }
        WM_CLOSE if !USER_CLOSE.load(Ordering::SeqCst) => {
            if let Some(on_close) = ON_CLOSE.get() {
                on_close();
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // SAFETY: removing our own subclass from a window of this thread, as the docs ask
            // before the window is destroyed.
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(dashboard_subclass), id) };
            // SAFETY: default processing with the original arguments.
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: default processing with the original arguments.
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}

/// `EnumThreadWindows` callback.
///
/// # Safety
/// Called by Windows during [`guard_helper_windows`], with `lparam` pointing to its counter.
unsafe extern "system" fn guard_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    if is_own_helper(hwnd) {
        // SAFETY: `hwnd` belongs to the calling thread (EnumThreadWindows), as SetWindowSubclass
        // requires; `close_subclass` lives for the whole program.
        if unsafe { SetWindowSubclass(hwnd, Some(close_subclass), SUBCLASS_ID, 0) }.as_bool() {
            // SAFETY: `lparam` is the counter pointer passed by `guard_helper_windows`, which is
            // still running.
            unsafe { *(lparam.0 as *mut usize) += 1 };
        }
    }
    TRUE
}

/// Whether `hwnd`'s class was registered by this executable and isn't a webview window. That
/// leaves out windows Windows itself gives the thread (e.g. the IME window).
fn is_own_helper(hwnd: HWND) -> bool {
    own_class_name(hwnd).is_some_and(|class| class != WEBVIEW_CLASS)
}

/// Whether `hwnd` is one of this executable's webview windows.
fn is_own_webview(hwnd: HWND) -> bool {
    own_class_name(hwnd).is_some_and(|class| class == WEBVIEW_CLASS)
}

/// The class name of `hwnd` if this executable registered its class.
fn own_class_name(hwnd: HWND) -> Option<String> {
    // SAFETY: returns this executable's module handle without transferring ownership.
    let module = unsafe { GetModuleHandleW(None) }.ok()?;
    // SAFETY: `hwnd` is a live window from EnumThreadWindows.
    if unsafe { GetClassLongPtrW(hwnd, GCLP_HMODULE) } != module.0 as usize {
        return None;
    }
    let mut buffer = [0u16; 64];
    // SAFETY: `buffer` is writable for its full length.
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let len = usize::try_from(len).unwrap_or(0).min(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
}

/// The subclass procedure.
///
/// # Safety
/// Called by Windows only, with valid subclass-procedure arguments.
unsafe extern "system" fn close_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    _data: usize,
) -> LRESULT {
    match msg {
        WM_CLOSE => {
            if let Some(on_close) = ON_CLOSE.get() {
                on_close();
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // SAFETY: removing our own subclass from a window of this thread, as the docs ask
            // before the window is destroyed.
            let _ = unsafe { RemoveWindowSubclass(hwnd, Some(close_subclass), id) };
            // SAFETY: default processing with the original arguments.
            unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: default processing with the original arguments.
        _ => unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) },
    }
}
