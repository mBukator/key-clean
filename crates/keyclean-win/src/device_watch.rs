//! Reports when input devices are connected or disconnected, with no input registration.
//!
//! Uses `CM_Register_Notification` for the keyboard, mouse and HID device interface classes
//! (ADR 0012). It tells the app that something changed; it carries no name, id or input data. The
//! app then re-lists devices (`devices::input_devices`). Registering for device interfaces doesn't
//! register for input, so idle KeyClean still sees no keystrokes (invariant 4).
//!
//! The callback runs on a thread-pool thread owned by Windows. It only sends on a channel. It
//! must return quickly and must never call `CM_Unregister_Notification`.
//! [docs] <https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_register_notification>

use std::ffi::c_void;
use std::sync::mpsc::Sender;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_NOTIFY_ACTION, CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL,
    CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL, CM_NOTIFY_EVENT_DATA, CM_NOTIFY_FILTER,
    CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE, CM_Register_Notification, CM_Unregister_Notification,
    CR_SUCCESS, HCMNOTIFICATION,
};
use windows::Win32::Devices::HumanInterfaceDevice::{
    GUID_DEVINTERFACE_HID, GUID_DEVINTERFACE_KEYBOARD, GUID_DEVINTERFACE_MOUSE,
};
use windows::core::GUID;

use crate::error::EngineError;

/// The interface classes watched. One registration covers one class. Keyboards on the PS/2 path
/// only show up under the keyboard class, USB and Bluetooth devices and precision touchpads under
/// HID.
const WATCHED_CLASSES: [GUID; 3] = [
    GUID_DEVINTERFACE_KEYBOARD,
    GUID_DEVINTERFACE_MOUSE,
    GUID_DEVINTERFACE_HID,
];

/// A running device watch. Dropping it stops the notifications.
pub struct DeviceWatch {
    registrations: Vec<HCMNOTIFICATION>,
    /// Passed to the callback as its context; freed after every registration is closed.
    context: *mut Sender<()>,
}

// SAFETY: the handles are only used to unregister (valid from any thread), and `context` points to
// a `Sender<()>`, which is `Send + Sync`, until `Drop` frees it.
unsafe impl Send for DeviceWatch {}
// SAFETY: as above; nothing is mutated through a shared reference.
unsafe impl Sync for DeviceWatch {}

impl DeviceWatch {
    /// Starts watching. Each connection or disconnection sends `()` on `changed`, possibly several
    /// times for one device (one per interface), so receivers should wait for quiet before they
    /// re-list.
    pub fn start(changed: Sender<()>) -> Result<DeviceWatch, EngineError> {
        let context = Box::into_raw(Box::new(changed));
        let mut watch = DeviceWatch {
            registrations: Vec::new(),
            context,
        };
        for class in WATCHED_CLASSES {
            let mut filter = CM_NOTIFY_FILTER {
                cbSize: size_of::<CM_NOTIFY_FILTER>() as u32,
                FilterType: CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE,
                ..Default::default()
            };
            filter.u.DeviceInterface.ClassGuid = class;
            let mut handle = HCMNOTIFICATION::default();
            // SAFETY: `filter` is fully initialized; `context` stays valid until `Drop` has closed
            // every registration; `handle` is writable.
            let result = unsafe {
                CM_Register_Notification(
                    &filter,
                    Some(context.cast_const().cast::<c_void>()),
                    Some(on_notification),
                    &mut handle,
                )
            };
            if result != CR_SUCCESS {
                // Dropping `watch` closes the registrations made so far.
                return Err(EngineError::DeviceWatch {
                    code: result.0 as i32,
                });
            }
            watch.registrations.push(handle);
        }
        Ok(watch)
    }
}

impl Drop for DeviceWatch {
    fn drop(&mut self) {
        let mut all_closed = true;
        for handle in self.registrations.drain(..) {
            // SAFETY: `handle` came from CM_Register_Notification and is closed once. This is not
            // called from the callback. It returns after any running callback has finished.
            all_closed &= unsafe { CM_Unregister_Notification(handle) } == CR_SUCCESS;
        }
        // If a registration couldn't be closed, its callback may still run, so the sender is left
        // allocated (a few bytes, once, at exit) rather than freed under it.
        if all_closed {
            // SAFETY: `context` came from Box::into_raw in `start`, and no callback can run any
            // more.
            drop(unsafe { Box::from_raw(self.context) });
        }
    }
}

/// Runs on a Windows thread-pool thread. Does nothing but send a signal.
///
/// # Safety
/// Called by Windows only. `context` is the `Sender<()>` passed at registration, which outlives
/// every registration.
unsafe extern "system" fn on_notification(
    _handle: HCMNOTIFICATION,
    context: *const c_void,
    action: CM_NOTIFY_ACTION,
    _event: *const CM_NOTIFY_EVENT_DATA,
    _event_size: u32,
) -> u32 {
    if action == CM_NOTIFY_ACTION_DEVICEINTERFACEARRIVAL
        || action == CM_NOTIFY_ACTION_DEVICEINTERFACEREMOVAL
    {
        // SAFETY: see the function's safety section.
        let sender = unsafe { &*context.cast::<Sender<()>>() };
        let _ = sender.send(());
    }
    // ERROR_SUCCESS: the only value the docs allow for these actions.
    0
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn starting_and_stopping_registers_nothing_that_outlives_the_watch() {
        // Read-only: device-interface notifications, no input registration.
        let (tx, _rx) = mpsc::channel();
        let watch = DeviceWatch::start(tx);
        assert!(watch.is_ok(), "{:?}", watch.err());
        drop(watch);
    }
}
