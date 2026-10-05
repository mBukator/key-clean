//! Whether the window input goes to belongs to an elevated (administrator) process: the foreground
//! window for keys, the window under the cursor for the mouse.
//!
//! A user-mode low-level hook may not be called for input that goes to a window of a higher
//! integrity level (UIPI), so input may reach an elevated window during a lock. The engine uses
//! this to tell that case apart from a hook Windows removed: the lock stays on and the user is
//! warned.
//!
//! The gap only exists when the foreground process is elevated and KeyClean isn't. An elevated
//! KeyClean (or a machine with UAC off, where every admin process is elevated) sees that input,
//! so a miss there is a real lost hook.

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::Security::{GetTokenInformation, TOKEN_ELEVATION, TOKEN_QUERY, TokenElevation};
use windows::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetWindowThreadProcessId, WindowFromPoint,
};

/// `Some(true)` if the foreground window's process is elevated, or its process or token can't be
/// opened (which is what an elevated process looks like from a standard one), while KeyClean's own
/// process is not elevated. `None` if no window is in the foreground (e.g. during a focus change or
/// a desktop switch): inconclusive, so the engine waits for the next keystroke to check again
/// rather than end the lock on a guess.
pub(crate) fn is_elevated_above_us() -> Option<bool> {
    // SAFETY: no preconditions; returns a null handle if no window is in the foreground.
    window_elevated_above_us(unsafe { GetForegroundWindow() })
}

/// Like [`is_elevated_above_us`], for the window under the cursor, which is where mouse input goes.
/// The position is used for this one lookup and not kept. `None` if the cursor position can't be
/// read or no window is under it.
pub(crate) fn is_elevated_under_cursor() -> Option<bool> {
    let mut point = POINT::default();
    // SAFETY: `point` is writable; failure (e.g. on another desktop) is an Err.
    unsafe { GetCursorPos(&mut point) }.ok()?;
    // SAFETY: no preconditions; returns a null handle if no window is at that point.
    window_elevated_above_us(unsafe { WindowFromPoint(point) })
}

fn window_elevated_above_us(hwnd: HWND) -> Option<bool> {
    if hwnd.is_invalid() {
        return None;
    }
    let mut pid = 0u32;
    // SAFETY: `pid` is writable; a window that just closed makes the call return 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    // SAFETY: opening a process by id has no memory-safety preconditions; failure is an Err.
    let Ok(process) = (unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) })
    else {
        return Some(true);
    };
    let elevated = token_elevated(process).unwrap_or(true);
    // SAFETY: `process` came from OpenProcess above and is closed once.
    let _ = unsafe { CloseHandle(process) };
    // SAFETY: GetCurrentProcess returns a pseudo-handle that needs no closing.
    let ours = token_elevated(unsafe { GetCurrentProcess() }).unwrap_or(false);
    Some(elevated && !ours)
}

fn token_elevated(process: HANDLE) -> Option<bool> {
    let mut token = HANDLE::default();
    // SAFETY: `process` is a valid process handle; `token` is writable.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
    let mut elevation = TOKEN_ELEVATION::default();
    let mut returned = 0u32;
    // SAFETY: the buffer is a TOKEN_ELEVATION, the structure TokenElevation returns, and the size
    // passed is its exact size; `returned` is writable.
    let result = unsafe {
        GetTokenInformation(
            token,
            TokenElevation,
            Some((&mut elevation as *mut TOKEN_ELEVATION).cast()),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        )
    };
    // SAFETY: `token` came from OpenProcessToken above and is closed once.
    let _ = unsafe { CloseHandle(token) };
    result.ok()?;
    Some(elevation.TokenIsElevated != 0)
}
