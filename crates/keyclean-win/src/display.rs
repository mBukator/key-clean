//! Keeps the display on while a lock's overlay is up (ADR 0015), so the screen doesn't dim or turn
//! off during a long cleaning session.
//!
//! Uses a power request object: `PowerCreateRequest`, then `PowerSetRequest` for
//! `PowerRequestDisplayRequired` and `PowerRequestSystemRequired`. The docs say a display request
//! needs a system request alongside it to keep the display on and the system out of idle sleep
//! [docs](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-powersetrequest).
//! Neither stops a sleep the user asks for: Windows ends both "upon user-initiated system sleep
//! entry (power button, lid close or selecting Sleep from the Start menu)" [docs, same page], so
//! the sleep exit of a lock still works (invariant 9).
//!
//! Dropping the guard clears both requests and closes the handle. If the process dies first,
//! Windows closes its handles [docs](https://learn.microsoft.com/en-us/windows/win32/procthread/terminating-a-process),
//! which frees the request object [assumption: the request ends with its object; checked in
//! `docs/testing/manual/M7.md` with `powercfg /requests`].

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::Power::{
    PowerClearRequest, PowerCreateRequest, PowerRequestDisplayRequired, PowerRequestSystemRequired,
    PowerSetRequest,
};
use windows::Win32::System::Threading::{
    POWER_REQUEST_CONTEXT_SIMPLE_STRING, REASON_CONTEXT, REASON_CONTEXT_0,
};
use windows::core::PWSTR;

/// `POWER_REQUEST_CONTEXT_VERSION` (0), defined here rather than enabling the windows crate's
/// whole `Win32_System_SystemServices` feature for one constant.
const POWER_REQUEST_CONTEXT_VERSION: u32 = 0;

/// Holds the display on until dropped.
#[derive(Debug)]
pub struct DisplayRequest {
    handle: HANDLE,
    system: bool,
}

// SAFETY: the handle is a kernel object handle owned by this value alone. Power request functions
// and `CloseHandle` may be called from any thread.
unsafe impl Send for DisplayRequest {}
// SAFETY: `&DisplayRequest` exposes no methods that use the handle.
unsafe impl Sync for DisplayRequest {}

impl DisplayRequest {
    /// Keeps the display on. `reason` is shown by `powercfg /requests` (the docs ask for a
    /// localized string). Fails if Windows refuses the display request; a refused system request
    /// alone is tolerated, since the display request is the part the user sees.
    pub fn acquire(reason: &str) -> windows::core::Result<DisplayRequest> {
        let mut wide: Vec<u16> = reason.encode_utf16().chain(Some(0)).collect();
        let context = REASON_CONTEXT {
            Version: POWER_REQUEST_CONTEXT_VERSION,
            Flags: POWER_REQUEST_CONTEXT_SIMPLE_STRING,
            Reason: REASON_CONTEXT_0 {
                SimpleReasonString: PWSTR(wide.as_mut_ptr()),
            },
        };
        // SAFETY: `context` and the NUL-terminated string it points to outlive the call, which
        // copies the reason. A failure is an Err.
        let handle = unsafe { PowerCreateRequest(&context) }?;
        // Owns the handle from here on, so an early return below closes it.
        let mut request = DisplayRequest {
            handle,
            system: false,
        };
        // SAFETY: `handle` is a live power request object owned by `request`.
        unsafe { PowerSetRequest(request.handle, PowerRequestDisplayRequired) }?;
        // SAFETY: as above.
        request.system =
            unsafe { PowerSetRequest(request.handle, PowerRequestSystemRequired) }.is_ok();
        Ok(request)
    }
}

impl Drop for DisplayRequest {
    fn drop(&mut self) {
        // SAFETY: `handle` is a live power request object owned by `self`; clearing a request that
        // was never set only returns an error, which is ignored.
        unsafe {
            if self.system {
                let _ = PowerClearRequest(self.handle, PowerRequestSystemRequired);
            }
            let _ = PowerClearRequest(self.handle, PowerRequestDisplayRequired);
            let _ = CloseHandle(self.handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Takes and releases a real request; it touches no input.
    #[test]
    fn acquire_and_release() {
        let request = DisplayRequest::acquire("KeyClean unit test").unwrap();
        drop(request);
    }
}
