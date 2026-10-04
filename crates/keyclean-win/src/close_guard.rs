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

use std::sync::OnceLock;

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, TRUE, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumThreadWindows, GCLP_HMODULE, GetClassLongPtrW, GetClassNameW, WM_CLOSE, WM_NCDESTROY,
};
use windows::core::BOOL;

/// Class of Tauri's webview windows, which handle `WM_CLOSE` themselves.
const WEBVIEW_CLASS: &str = "Tauri Window";
/// Subclass id; any constant unique to this module.
const SUBCLASS_ID: usize = 0x4B43_434C;

static ON_CLOSE: OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();

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
    // SAFETY: returns this executable's module handle without transferring ownership.
    let Ok(module) = (unsafe { GetModuleHandleW(None) }) else {
        return false;
    };
    // SAFETY: `hwnd` is a live window from EnumThreadWindows.
    if unsafe { GetClassLongPtrW(hwnd, GCLP_HMODULE) } != module.0 as usize {
        return false;
    }
    let mut buffer = [0u16; 64];
    // SAFETY: `buffer` is writable for its full length.
    let len = unsafe { GetClassNameW(hwnd, &mut buffer) };
    let len = usize::try_from(len).unwrap_or(0).min(buffer.len());
    String::from_utf16_lossy(&buffer[..len]) != WEBVIEW_CLASS
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
