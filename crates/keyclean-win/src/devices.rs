//! Keyboard enumeration through Raw Input and the configuration manager.
//!
//! Listing devices doesn't register for input, so KeyClean still sees no keystrokes while idle
//! (invariant 4). Names come from `DEVPKEY_Device_FriendlyName`, falling back to `DEVPKEY_NAME`.
//! In M1 the list is informational: every keyboard is locked (ADR 0004).

use std::mem::size_of;

use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_PropertyW, CM_LOCATE_DEVNODE_NORMAL,
    CM_Locate_DevNodeW, CR_BUFFER_SMALL, CR_SUCCESS,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_FriendlyName, DEVPKEY_Device_InstanceId, DEVPKEY_NAME, DEVPROP_TYPE_STRING,
    DEVPROPTYPE,
};
use windows::Win32::Foundation::{DEVPROPKEY, ERROR_INSUFFICIENT_BUFFER, GetLastError, HANDLE};
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RIDI_DEVICENAME,
    RIM_TYPEKEYBOARD,
};
use windows::core::PCWSTR;

use crate::error::EngineError;

/// A connected keyboard. Contains no input data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyboardDevice {
    /// The Raw Input device interface path; stable while the device stays connected.
    pub id: String,
    /// The name Windows shows for the device, when it has one.
    pub name: Option<String>,
}

/// Lists the connected keyboards.
pub fn keyboards() -> Result<Vec<KeyboardDevice>, EngineError> {
    let mut keyboards = Vec::new();
    for device in raw_input_devices()? {
        if device.dwType != RIM_TYPEKEYBOARD {
            continue;
        }
        let Some(id) = device_path(device.hDevice) else {
            continue;
        };
        let name = friendly_name(&id);
        keyboards.push(KeyboardDevice { id, name });
    }
    Ok(keyboards)
}

pub(crate) fn raw_input_devices() -> Result<Vec<RAWINPUTDEVICELIST>, EngineError> {
    let entry_size = size_of::<RAWINPUTDEVICELIST>() as u32;
    // A device can arrive between the two calls; retry a few times.
    for _ in 0..3 {
        let mut count = 0u32;
        // SAFETY: a null list pointer asks for the device count only.
        if unsafe { GetRawInputDeviceList(None, &mut count, entry_size) } == u32::MAX {
            return Err(last_error());
        }
        let mut list = vec![RAWINPUTDEVICELIST::default(); count as usize];
        // SAFETY: `list` has room for `count` entries of `entry_size` bytes.
        let written =
            unsafe { GetRawInputDeviceList(Some(list.as_mut_ptr()), &mut count, entry_size) };
        if written == u32::MAX {
            // SAFETY: GetLastError has no preconditions.
            if unsafe { GetLastError() } == ERROR_INSUFFICIENT_BUFFER {
                continue;
            }
            return Err(last_error());
        }
        list.truncate(written as usize);
        return Ok(list);
    }
    Err(EngineError::DeviceQuery {
        code: ERROR_INSUFFICIENT_BUFFER.0 as i32,
        message: "device list kept changing".into(),
    })
}

/// Raw Input handles of the connected keyboards, without names (no configuration-manager calls,
/// so it is cheap enough for the engine thread).
pub(crate) fn keyboard_handles() -> Result<Vec<isize>, EngineError> {
    Ok(raw_input_devices()?
        .into_iter()
        .filter(|device| device.dwType == RIM_TYPEKEYBOARD)
        .map(|device| device.hDevice.0 as isize)
        .collect())
}

fn last_error() -> EngineError {
    EngineError::device_query(&windows::core::Error::from_thread())
}

fn device_path(device: HANDLE) -> Option<String> {
    let mut chars = 0u32;
    // SAFETY: a null buffer asks for the required size, in characters for RIDI_DEVICENAME.
    unsafe { GetRawInputDeviceInfoW(Some(device), RIDI_DEVICENAME, None, &mut chars) };
    if chars == 0 {
        return None;
    }
    let mut buffer = vec![0u16; chars as usize];
    // SAFETY: `buffer` holds `chars` UTF-16 units, matching the size passed in.
    let copied = unsafe {
        GetRawInputDeviceInfoW(
            Some(device),
            RIDI_DEVICENAME,
            Some(buffer.as_mut_ptr().cast()),
            &mut chars,
        )
    };
    if copied == u32::MAX || copied == 0 {
        return None;
    }
    Some(utf16_until_nul(&buffer))
}

fn friendly_name(interface_path: &str) -> Option<String> {
    let path = to_wide(interface_path);
    let instance_id = interface_string_property(&path, &DEVPKEY_Device_InstanceId)?;
    let instance_id = to_wide(&instance_id);

    let mut devinst = 0u32;
    // SAFETY: `instance_id` is NUL-terminated and outlives the call; `devinst` is writable.
    let located = unsafe {
        CM_Locate_DevNodeW(
            &mut devinst,
            PCWSTR(instance_id.as_ptr()),
            CM_LOCATE_DEVNODE_NORMAL,
        )
    };
    if located != CR_SUCCESS {
        return None;
    }
    devnode_string_property(devinst, &DEVPKEY_Device_FriendlyName)
        .or_else(|| devnode_string_property(devinst, &DEVPKEY_NAME))
}

fn interface_string_property(path: &[u16], key: &DEVPROPKEY) -> Option<String> {
    read_string_property(|prop_type, buffer, size| {
        // SAFETY: `path` is NUL-terminated; the buffer/size pair comes from `read_string_property`
        // and describes valid writable memory (or a size query when the buffer is None).
        unsafe {
            CM_Get_Device_Interface_PropertyW(
                PCWSTR(path.as_ptr()),
                key,
                prop_type,
                buffer,
                size,
                0,
            )
        }
    })
}

fn devnode_string_property(devinst: u32, key: &DEVPROPKEY) -> Option<String> {
    read_string_property(|prop_type, buffer, size| {
        // SAFETY: as above; `devinst` came from CM_Locate_DevNodeW.
        unsafe { CM_Get_DevNode_PropertyW(devinst, key, prop_type, buffer, size, 0) }
    })
}

/// Runs the configuration manager's two-call pattern for a string property.
fn read_string_property(
    mut query: impl FnMut(
        *mut DEVPROPTYPE,
        Option<*mut u8>,
        *mut u32,
    ) -> windows::Win32::Devices::DeviceAndDriverInstallation::CONFIGRET,
) -> Option<String> {
    let mut prop_type = DEVPROPTYPE::default();
    let mut size = 0u32;
    let first = query(&mut prop_type, None, &mut size);
    if first != CR_BUFFER_SMALL || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    if query(&mut prop_type, Some(buffer.as_mut_ptr()), &mut size) != CR_SUCCESS
        || prop_type != DEVPROP_TYPE_STRING
    {
        return None;
    }
    let units: Vec<u16> = buffer
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u16::from_le_bytes(*pair))
        .collect();
    let text = utf16_until_nul(&units);
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn utf16_until_nul(units: &[u16]) -> String {
    let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_helpers() {
        let wide = to_wide("HID Keyboard Device");
        assert_eq!(wide.last(), Some(&0));
        assert_eq!(utf16_until_nul(&wide), "HID Keyboard Device");
        assert_eq!(utf16_until_nul(&[0x41, 0x42]), "AB");
    }

    #[test]
    fn listing_keyboards_does_not_fail() {
        // Read-only enumeration; installs nothing and registers for no input.
        let result = keyboards();
        assert!(result.is_ok(), "{result:?}");
    }
}
