//! Input device enumeration through Raw Input and the configuration manager.
//!
//! Listing devices doesn't register for input, so KeyClean still sees no keystrokes while idle
//! (invariant 4). Names come from `DEVPKEY_Device_FriendlyName`, falling back to `DEVPKEY_NAME`.
//! The list is informational: every keyboard is locked, nothing else yet (ADR 0004). What counts
//! as a keyboard, mouse or touchpad, and how far KeyClean controls it, is decided in
//! `keyclean_core::devices`.

use std::mem::size_of;

use keyclean_core::devices::{InputDevice, RawDevice, RawKind, build_list};
use windows::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Get_Device_Interface_PropertyW, CM_Get_Parent,
    CM_LOCATE_DEVNODE_NORMAL, CM_Locate_DevNodeW, CR_BUFFER_SMALL, CR_SUCCESS,
};
use windows::Win32::Devices::Properties::{
    DEVPKEY_Device_BusReportedDeviceDesc, DEVPKEY_Device_ContainerId, DEVPKEY_Device_FriendlyName,
    DEVPKEY_Device_InLocalMachineContainer, DEVPKEY_Device_InstanceId, DEVPKEY_NAME,
    DEVPROP_TYPE_BOOLEAN, DEVPROP_TYPE_GUID, DEVPROP_TYPE_STRING, DEVPROPTYPE,
};
use windows::Win32::Foundation::{DEVPROPKEY, ERROR_INSUFFICIENT_BUFFER, GetLastError, HANDLE};
use windows::Win32::UI::Input::{
    GetRawInputDeviceInfoW, GetRawInputDeviceList, RAWINPUTDEVICELIST, RID_DEVICE_INFO,
    RIDI_DEVICEINFO, RIDI_DEVICENAME, RIM_TYPEHID, RIM_TYPEKEYBOARD, RIM_TYPEMOUSE,
};
use windows::core::PCWSTR;

use crate::error::EngineError;

/// Lists the connected keyboards, mice, touchpads and other pointing devices KeyClean knows about
/// (see `keyclean_core::devices`). Registers for no input.
pub fn input_devices() -> Result<Vec<InputDevice>, EngineError> {
    let mut raw = Vec::new();
    for device in raw_input_devices()? {
        let Some(kind) = raw_kind(device.hDevice, device.dwType) else {
            continue;
        };
        let Some(id) = device_path(device.hDevice) else {
            continue;
        };
        let devnode = locate_devnode(&id);
        let container = devnode.and_then(external_container);
        // An external device is better described by the name its bus reports ("HyperX Alloy
        // Origins") than by the generic class name ("HID Keyboard Device").
        let name = devnode.and_then(|devnode| {
            container
                .as_deref()
                .and_then(|container| product_name(devnode, container))
                .or_else(|| friendly_name(devnode))
        });
        raw.push(RawDevice {
            name,
            group: devnode.and_then(parent_devnode).map(u64::from),
            container,
            id,
            kind,
        });
    }
    Ok(build_list(raw))
}

/// What Raw Input says a device is. HID devices that aren't keyboards or mice need their usage
/// from `RIDI_DEVICEINFO`; if that can't be read the device is skipped.
fn raw_kind(
    device: HANDLE,
    device_type: windows::Win32::UI::Input::RID_DEVICE_INFO_TYPE,
) -> Option<RawKind> {
    if device_type == RIM_TYPEKEYBOARD {
        return Some(RawKind::Keyboard);
    }
    if device_type == RIM_TYPEMOUSE {
        return Some(RawKind::Mouse);
    }
    if device_type != RIM_TYPEHID {
        return None;
    }
    let mut info = RID_DEVICE_INFO {
        cbSize: size_of::<RID_DEVICE_INFO>() as u32,
        ..Default::default()
    };
    let mut size = info.cbSize;
    // SAFETY: `info` is a RID_DEVICE_INFO whose cbSize is set, and `size` is its size in bytes, as
    // RIDI_DEVICEINFO requires.
    let copied = unsafe {
        GetRawInputDeviceInfoW(
            Some(device),
            RIDI_DEVICEINFO,
            Some((&raw mut info).cast()),
            &mut size,
        )
    };
    if copied == u32::MAX || copied == 0 {
        return None;
    }
    // SAFETY: dwType is RIM_TYPEHID, so the `hid` member of the union is the one Windows filled in.
    let hid = unsafe { info.Anonymous.hid };
    Some(RawKind::Hid {
        usage_page: hid.usUsagePage,
        usage: hid.usUsage,
    })
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

/// The configuration-manager device node behind a Raw Input interface path.
fn locate_devnode(interface_path: &str) -> Option<u32> {
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
    (located == CR_SUCCESS).then_some(devinst)
}

fn friendly_name(devinst: u32) -> Option<String> {
    devnode_string_property(devinst, &DEVPKEY_Device_FriendlyName)
        .or_else(|| devnode_string_property(devinst, &DEVPKEY_NAME))
}

/// The container (physical device) of a device that isn't built into this machine, as text.
/// Built-in devices all share one machine container, which says nothing about which are the same
/// device, so they get `None`. [tested] `DEVPKEY_Device_InLocalMachineContainer` is false for USB
/// devices and true for the internal keyboard and touchpad on Max's machine.
fn external_container(devinst: u32) -> Option<String> {
    let in_machine = devnode_bytes(
        devinst,
        &DEVPKEY_Device_InLocalMachineContainer,
        DEVPROP_TYPE_BOOLEAN,
    )?;
    if in_machine.first() != Some(&0) {
        return None;
    }
    let id = devnode_bytes(devinst, &DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID)?;
    Some(id.iter().map(|b| format!("{b:02x}")).collect())
}

/// The product name the bus reported for the device or an ancestor in the same container (for a
/// USB keyboard: the composite device above its interfaces). A hub or controller above it has a
/// different container, so its name is never used.
fn product_name(devinst: u32, container: &str) -> Option<String> {
    let mut node = devinst;
    for _ in 0..4 {
        let same_container = devnode_bytes(node, &DEVPKEY_Device_ContainerId, DEVPROP_TYPE_GUID)
            .is_some_and(|id| {
                id.iter().map(|b| format!("{b:02x}")).collect::<String>() == container
            });
        if !same_container {
            return None;
        }
        if let Some(name) = devnode_string_property(node, &DEVPKEY_Device_BusReportedDeviceDesc) {
            return Some(name);
        }
        node = parent_devnode(node)?;
    }
    None
}

/// The parent device node. Collections of one HID device (a touchpad and its companion mouse)
/// share it.
fn parent_devnode(devinst: u32) -> Option<u32> {
    let mut parent = 0u32;
    // SAFETY: `devinst` came from CM_Locate_DevNodeW and `parent` is writable.
    (unsafe { CM_Get_Parent(&mut parent, devinst, 0) } == CR_SUCCESS).then_some(parent)
}

fn interface_string_property(path: &[u16], key: &DEVPROPKEY) -> Option<String> {
    let bytes = read_property_bytes(
        |prop_type, buffer, size| {
            // SAFETY: `path` is NUL-terminated; the buffer/size pair comes from
            // `read_property_bytes` and describes valid writable memory (or a size query when the
            // buffer is None).
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
        },
        DEVPROP_TYPE_STRING,
    )?;
    string_from_bytes(&bytes)
}

fn devnode_string_property(devinst: u32, key: &DEVPROPKEY) -> Option<String> {
    string_from_bytes(&devnode_bytes(devinst, key, DEVPROP_TYPE_STRING)?)
}

/// Runs the configuration manager's two-call pattern for a property of the expected type and
/// returns its bytes.
fn read_property_bytes(
    mut query: impl FnMut(
        *mut DEVPROPTYPE,
        Option<*mut u8>,
        *mut u32,
    ) -> windows::Win32::Devices::DeviceAndDriverInstallation::CONFIGRET,
    expected: DEVPROPTYPE,
) -> Option<Vec<u8>> {
    let mut prop_type = DEVPROPTYPE::default();
    let mut size = 0u32;
    let first = query(&mut prop_type, None, &mut size);
    if first != CR_BUFFER_SMALL || size == 0 {
        return None;
    }
    let mut buffer = vec![0u8; size as usize];
    if query(&mut prop_type, Some(buffer.as_mut_ptr()), &mut size) != CR_SUCCESS
        || prop_type != expected
    {
        return None;
    }
    buffer.truncate(size as usize);
    Some(buffer)
}

/// A property of a device node, as bytes, if it has one of the expected type.
fn devnode_bytes(devinst: u32, key: &DEVPROPKEY, expected: DEVPROPTYPE) -> Option<Vec<u8>> {
    read_property_bytes(
        |prop_type, buffer, size| {
            // SAFETY: the buffer/size pair comes from `read_property_bytes` and describes valid
            // writable memory (or a size query when the buffer is None); `devinst` came from
            // CM_Locate_DevNodeW or CM_Get_Parent.
            unsafe { CM_Get_DevNode_PropertyW(devinst, key, prop_type, buffer, size, 0) }
        },
        expected,
    )
}

/// Decodes a string property: trimmed, and `None` when empty.
fn string_from_bytes(buffer: &[u8]) -> Option<String> {
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

    /// Diagnostic: prints the list as the window would show it. Read-only (no hook, no input
    /// registration). Run: `cargo test -p keyclean-win print_devices -- --ignored --nocapture`.
    #[test]
    #[ignore = "diagnostic output"]
    fn print_devices() {
        for d in input_devices().unwrap() {
            println!("{:?} {:?} {:?}: {}", d.kind, d.capability, d.name, d.id);
        }
    }

    #[test]
    fn listing_devices_does_not_fail() {
        // Read-only enumeration; installs nothing and registers for no input.
        let result = input_devices();
        assert!(result.is_ok(), "{result:?}");
    }
}
