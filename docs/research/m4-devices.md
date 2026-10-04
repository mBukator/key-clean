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

## Duplicates

- A touchpad's optional mouse-mode collection can appear as a second Raw Input device, and
  touchscreens and pens may expose one too. The list folds a `RIM_TYPEMOUSE` entry into a touchpad,
  touchscreen or pen when both have the same **parent device node**
  (`CM_Get_Parent`). [assumption: collections of one HID device share its device node as parent]
  [needs hardware]
- Nothing else is merged. Folding by `DEVPKEY_Device_ContainerId` was considered and left out:
  the container ID groups the device nodes of one physical device [docs], but the docs I found don't
  say whether devices built into the machine share one container, and merging two real internal
  devices into one row is worse than a duplicate row.
  https://learn.microsoft.com/en-us/windows-hardware/drivers/install/how-container-ids-are-generated
- Known consequence: one physical keyboard that exposes several keyboard interfaces (for example a
  boot keyboard plus an N-key-rollover one) may appear as more than one row. [needs hardware] M4.md
  records what Max's machine shows; container folding can follow once there is data.

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
