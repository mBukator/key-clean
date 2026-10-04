# M4 device detection research notes

Findings behind the M4 device list. Claims are tagged [docs], [tested] or [assumption]; "[needs
hardware]" means the M4 manual test records what Max's machine reports.

## Classifying a device

- `GetRawInputDeviceList` returns every Raw Input device with its type: `RIM_TYPEMOUSE` (0),
  `RIM_TYPEKEYBOARD` (1), `RIM_TYPEHID` (2, a HID that is not a keyboard or mouse). `RIDI_DEVICEINFO`
  adds the HID usage page and usage for the HID type. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/winuser/ns-winuser-rid_device_info
- Windows opens these HID top-level collections for system use (usage page / usage): mouse
  0x01/0x01-0x02, keyboard 0x01/0x06-0x07, external pen 0x0D/0x01, integrated pen 0x0D/0x02,
  touchscreen 0x0D/0x04, precision touchpad 0x0D/0x05. [docs]
  https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/top-level-collections-opened-by-windows-for-system-use
- A precision touchpad exposes a touchpad collection and a configuration collection, plus optional
  firmware-update and basic mouse-mode collections. [docs]
  https://learn.microsoft.com/en-us/windows-hardware/design/component-guidelines/touchpad-required-hid-top-level-collections
- Capability model (ADR 0004): keyboards and mice **Supported**, precision touchpads **Limited**,
  touchscreens and pens **Unsupported**. Other HID collections (consumer control, sensors, vendor
  pages) are not input devices KeyClean cares about and aren't listed.
- A touchpad that is not a precision touchpad (older PS/2 or I2C mouse-class pads) is a mouse to
  Windows and to the M5 hook, so it is listed as a mouse. There is no guessing from names.
  [assumption]

## Duplicates and names

What Max's machine reported (2026-10-04, `Get-PnpDevice` and the first Part B list) [tested]:

- A USB keyboard (HyperX Alloy Origins, `VID_03F0&PID_0591`) exposes **two keyboard interfaces
  (MI_00, MI_02) and one mouse interface (MI_01)**. Each interface has its own device node, all under
  one USB composite device, all in one container. Raw Input listed them as three devices, the window
  as three rows ("HID Keyboard Device" twice, "HID-compliant mouse").
- A Logitech receiver (`VID_046D&PID_C548`) exposes a keyboard, a mouse and a **digitizer
  collection with the precision-touchpad usage** (MI_03, "HID-compliant touch pad"). That is a
  second "touchpad" row, although no touchpad is attached to the machine through it. The list shows
  what Windows reports; the row is named "USB Receiver", so it is recognisable.
- The laptop's own touchpad (ELAN1203) has a touchpad collection (Col02) and a mouse collection
  (Col01) under **one parent device node** (`ACPI\ELAN1203`). Folding the mouse into the touchpad
  by parent worked.
- Built-in devices (the PS/2 keyboard, the ELAN touchpad) are all in the **machine container**
  `{00000000-0000-0000-FFFF-FFFFFFFFFFFF}`, and `DEVPKEY_Device_InLocalMachineContainer` is true for
  them and false for the USB devices. So the container can't tell built-in devices apart, and the
  property is the test for "built in".
- The USB composite device's `DEVPKEY_Device_BusReportedDeviceDesc` is the product name ("HyperX
  Alloy Origins", "USB Receiver"). The HID interface nodes only say "HID Keyboard Device".

What the list does with that:

- A `RIM_TYPEMOUSE` entry folds into a touchpad, touchscreen or pen with the same **parent device
  node** (`CM_Get_Parent`).
- Entries of the **same kind in the same external container** become one row (the HyperX's two
  keyboard interfaces). Built-in devices are never folded by container: they have none to compare,
  and two real internal devices must not become one row. Entries are never merged by name.
- For an external device the name is the first `BusReportedDeviceDesc` found on the device or an
  ancestor **in the same container** (a hub or controller above it has another container, so its
  name is never used); otherwise the usual friendly name.
- A keyboard with a mouse interface also appears under Mice, with the same name. That is what
  Windows exposes, and it is what a mouse lock (M5) will block.
- Not known yet: whether Bluetooth and other buses report the same properties. [needs hardware]
  If the bus reports nothing, the friendly name is used.

## Watching for changes

- `CM_Register_Notification` registers a callback for PnP events, available from Windows 8. A
  device-interface filter (`CM_NOTIFY_FILTER_TYPE_DEVICEINTERFACE`) takes one interface class
  GUID per registration. [docs]
  https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_register_notification
- The docs ask for handlers to return as quickly as possible and to hand blocking work to another
  thread; for actions other than query-remove the callback returns `ERROR_SUCCESS`. [docs]
- The registration reports no devices that already exist. Register first, then list, so a device
  connected in between is not missed. [docs]
- Registrations are closed with `CM_Unregister_Notification`, which waits for a running callback
  to finish, so the callback must not call it. [docs for closing; the callback restriction follows
  from the waiting and is kept as an [assumption]] If a registration can't be closed, the sender
  its callback uses is left allocated instead of freed.
- Classes watched: `GUID_DEVINTERFACE_KEYBOARD`, `GUID_DEVINTERFACE_MOUSE`,
  `GUID_DEVINTERFACE_HID` (constants from the `windows` crate). PS/2 devices appear only under
  the keyboard and mouse classes, USB and Bluetooth devices and precision touchpads under HID.
  [assumption] [needs hardware]
- One connection fires several notifications, and friendly names may not be readable straight
  away. The app waits for 400 ms of quiet before listing again, and reads again up to twice, 1 s
  apart, while a listed device still has no name. [assumption, checked by M4 steps 3-6]
- Whether notifications arrive after resume for devices changed during sleep is not documented.
  [needs hardware] (M4.md step 8)

## Idle cost and privacy

- Device interface notifications are not Raw Input registrations: the idle check (harness S22,
  `GetRegisteredRawInputDevices` count) is unaffected. [assumption, rerun in M4]
- The refresh thread blocks on a channel; nothing runs while nothing changes.
- The list shows names and ids (that is its purpose). Logs get counts only.
