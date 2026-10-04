//! The device model (§23, §24): what kind of input device something is, how far KeyClean can
//! control it (ADR 0004), and how raw device entries from Windows become the list the user sees.
//!
//! Pure logic only. The Windows side gathers [`RawDevice`] facts; nothing here touches input data.

/// The HID usage page for digitizers (pens, touchscreens, touchpads).
/// [docs] <https://learn.microsoft.com/en-us/windows-hardware/drivers/hid/top-level-collections-opened-by-windows-for-system-use>
const USAGE_PAGE_DIGITIZER: u16 = 0x000D;
/// Digitizer usage: external pen device.
const USAGE_PEN_EXTERNAL: u16 = 0x0001;
/// Digitizer usage: integrated pen device.
const USAGE_PEN_INTEGRATED: u16 = 0x0002;
/// Digitizer usage: touchscreen.
const USAGE_TOUCH_SCREEN: u16 = 0x0004;
/// Digitizer usage: Windows Precision Touchpad.
const USAGE_PRECISION_TOUCHPAD: u16 = 0x0005;

/// What a Raw Input entry says it is, before classification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawKind {
    /// `RIM_TYPEKEYBOARD`.
    Keyboard,
    /// `RIM_TYPEMOUSE`.
    Mouse,
    /// `RIM_TYPEHID`: some other HID top-level collection, identified by its usage.
    Hid {
        /// HID usage page.
        usage_page: u16,
        /// HID usage within the page.
        usage: u16,
    },
}

/// The kinds of device the list shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeviceKind {
    Keyboard,
    Mouse,
    Touchpad,
    Touchscreen,
    Pen,
}

/// How far KeyClean can control a device (§23, ADR 0004).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    /// KeyClean can block it.
    Supported,
    /// KeyClean can block some of what it does, with caveats (precision touchpads).
    Limited,
    /// KeyClean can't control it, and says so.
    Unsupported,
}

/// A device entry as Windows reports it. Carries no input data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawDevice {
    /// The Raw Input device interface path; stable while the device stays connected.
    pub id: String,
    /// The name Windows shows for the device, when it has one.
    pub name: Option<String>,
    /// What Raw Input says it is.
    pub kind: RawKind,
    /// Identifies the parent device node. Entries with the same group are collections of one HID
    /// device (for example a touchpad and its optional companion mouse collection).
    pub group: Option<u64>,
    /// Identifies the physical device (its container) for devices that aren't built into the
    /// machine. `None` for built-in devices and when it can't be read. Entries of the same kind in
    /// one container are interfaces of one device.
    pub container: Option<String>,
}

/// A device in the list shown to the user.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InputDevice {
    /// The Raw Input device interface path.
    pub id: String,
    /// The name Windows shows for the device, when it has one.
    pub name: Option<String>,
    /// What kind of device it is.
    pub kind: DeviceKind,
    /// How far KeyClean can control it.
    pub capability: Capability,
}

/// Kind and capability for a raw entry, or `None` for HID collections that aren't input devices
/// KeyClean cares about (consumer controls, sensors, vendor pages, ...).
///
/// - Keyboards and mice: **Supported**.
/// - Precision touchpads: **Limited**. Their input reaches applications as mouse input, the
///   mouse hook can't tell them from a mouse, and system gestures may bypass it (ADR 0004).
/// - Touchscreens and pens: **Unsupported**.
pub fn classify(kind: RawKind) -> Option<(DeviceKind, Capability)> {
    match kind {
        RawKind::Keyboard => Some((DeviceKind::Keyboard, Capability::Supported)),
        RawKind::Mouse => Some((DeviceKind::Mouse, Capability::Supported)),
        RawKind::Hid {
            usage_page: USAGE_PAGE_DIGITIZER,
            usage,
        } => match usage {
            USAGE_PRECISION_TOUCHPAD => Some((DeviceKind::Touchpad, Capability::Limited)),
            USAGE_TOUCH_SCREEN => Some((DeviceKind::Touchscreen, Capability::Unsupported)),
            USAGE_PEN_EXTERNAL | USAGE_PEN_INTEGRATED => {
                Some((DeviceKind::Pen, Capability::Unsupported))
            }
            _ => None,
        },
        RawKind::Hid { .. } => None,
    }
}

/// Turns raw entries into the list to show: classified, with duplicates of one physical device
/// folded, and in a stable order (by kind, then name, then id).
///
/// Two things are folded:
/// - the companion mouse collection of a touchpad, touchscreen or pen (same parent device node);
/// - entries of the same kind in the same external container: one USB keyboard that exposes two
///   keyboard interfaces is one row. Built-in devices have no container here, so two of them are
///   never merged, however alike they look.
///
/// Entries are never merged by name.
pub fn build_list(raw: Vec<RawDevice>) -> Vec<InputDevice> {
    let digitizer_groups: Vec<u64> = raw
        .iter()
        .filter(|d| {
            matches!(
                classify(d.kind),
                Some((
                    DeviceKind::Touchpad | DeviceKind::Touchscreen | DeviceKind::Pen,
                    _
                ))
            )
        })
        .filter_map(|d| d.group)
        .collect();

    let mut entries: Vec<(InputDevice, Option<String>)> = raw
        .into_iter()
        .filter_map(|device| {
            let (kind, capability) = classify(device.kind)?;
            let companion_mouse = kind == DeviceKind::Mouse
                && device.group.is_some_and(|g| digitizer_groups.contains(&g));
            (!companion_mouse).then_some((
                InputDevice {
                    id: device.id,
                    name: device.name,
                    kind,
                    capability,
                },
                device.container,
            ))
        })
        .collect();

    // The entry with the lowest id represents its container, so the choice doesn't depend on the
    // order Windows lists devices in. A name found on another entry fills in a missing one.
    entries.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    let mut list: Vec<InputDevice> = Vec::with_capacity(entries.len());
    let mut seen: Vec<(String, DeviceKind, usize)> = Vec::new();
    for (device, container) in entries {
        let Some(container) = container else {
            list.push(device);
            continue;
        };
        match seen
            .iter()
            .find(|(c, kind, _)| *c == container && *kind == device.kind)
        {
            Some(&(_, _, index)) => {
                if list[index].name.is_none() {
                    list[index].name = device.name;
                }
            }
            None => {
                seen.push((container, device.kind, list.len()));
                list.push(device);
            }
        }
    }
    list.sort_by_cached_key(|d| {
        (
            d.kind,
            d.name.as_deref().unwrap_or_default().to_lowercase(),
            d.id.clone(),
        )
    });
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hid(usage_page: u16, usage: u16) -> RawKind {
        RawKind::Hid { usage_page, usage }
    }

    fn raw(id: &str, name: Option<&str>, kind: RawKind, group: Option<u64>) -> RawDevice {
        RawDevice {
            id: id.into(),
            name: name.map(str::to_owned),
            kind,
            group,
            container: None,
        }
    }

    fn in_container(mut device: RawDevice, container: &str) -> RawDevice {
        device.container = Some(container.to_owned());
        device
    }

    #[test]
    fn keyboards_and_mice_are_supported() {
        assert_eq!(
            classify(RawKind::Keyboard),
            Some((DeviceKind::Keyboard, Capability::Supported))
        );
        assert_eq!(
            classify(RawKind::Mouse),
            Some((DeviceKind::Mouse, Capability::Supported))
        );
    }

    #[test]
    fn precision_touchpads_are_limited() {
        assert_eq!(
            classify(hid(0x0D, 0x05)),
            Some((DeviceKind::Touchpad, Capability::Limited))
        );
    }

    #[test]
    fn touchscreens_and_pens_are_unsupported() {
        let unsupported = |kind| Some((kind, Capability::Unsupported));
        assert_eq!(
            classify(hid(0x0D, 0x04)),
            unsupported(DeviceKind::Touchscreen)
        );
        assert_eq!(classify(hid(0x0D, 0x01)), unsupported(DeviceKind::Pen));
        assert_eq!(classify(hid(0x0D, 0x02)), unsupported(DeviceKind::Pen));
    }

    #[test]
    fn other_hid_collections_are_ignored() {
        // Consumer control, system control, vendor-defined, a digitizer usage that isn't listed,
        // and the generic-desktop usages of mice and keyboards (those arrive as their own types).
        for (page, usage) in [
            (0x0C, 0x01),
            (0x01, 0x80),
            (0xFF00, 0x01),
            (0x0D, 0x03),
            (0x01, 0x02),
        ] {
            assert_eq!(classify(hid(page, usage)), None, "{page:#x}/{usage:#x}");
        }
    }

    #[test]
    fn companion_mouse_collection_folds_into_the_touchpad() {
        let list = build_list(vec![
            raw(
                "mouse",
                Some("HID-compliant mouse"),
                RawKind::Mouse,
                Some(7),
            ),
            raw("pad", Some("Touchpad"), hid(0x0D, 0x05), Some(7)),
        ]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, DeviceKind::Touchpad);
        assert_eq!(list[0].capability, Capability::Limited);
    }

    #[test]
    fn companion_mouse_collection_folds_into_a_touchscreen_or_pen() {
        let list = build_list(vec![
            raw("m1", Some("HID-compliant mouse"), RawKind::Mouse, Some(1)),
            raw("screen", Some("Touch screen"), hid(0x0D, 0x04), Some(1)),
            raw("m2", Some("HID-compliant mouse"), RawKind::Mouse, Some(2)),
            raw("pen", Some("Pen"), hid(0x0D, 0x02), Some(2)),
        ]);
        let kinds: Vec<_> = list.iter().map(|d| d.kind).collect();
        assert_eq!(kinds, [DeviceKind::Touchscreen, DeviceKind::Pen]);
    }

    #[test]
    fn mice_in_other_groups_or_without_a_group_stay() {
        let list = build_list(vec![
            raw("pad", Some("Touchpad"), hid(0x0D, 0x05), Some(7)),
            raw("usb", Some("USB mouse"), RawKind::Mouse, Some(8)),
            raw("unknown", Some("Mystery mouse"), RawKind::Mouse, None),
        ]);
        let kinds: Vec<_> = list.iter().map(|d| d.kind).collect();
        assert_eq!(
            kinds,
            [DeviceKind::Mouse, DeviceKind::Mouse, DeviceKind::Touchpad]
        );
    }

    #[test]
    fn built_in_devices_are_never_merged() {
        // Same name, kind and group, but no container: two real devices must stay two rows.
        let list = build_list(vec![
            raw("a", Some("HID Keyboard Device"), RawKind::Keyboard, Some(1)),
            raw("b", Some("HID Keyboard Device"), RawKind::Keyboard, Some(1)),
        ]);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn interfaces_of_one_external_device_become_one_row_per_kind() {
        // A USB keyboard with two keyboard interfaces and a mouse interface (one container).
        let list = build_list(vec![
            in_container(raw("kb2", Some("Board"), RawKind::Keyboard, Some(3)), "C1"),
            in_container(raw("mouse", Some("Board"), RawKind::Mouse, Some(2)), "C1"),
            in_container(raw("kb1", Some("Board"), RawKind::Keyboard, Some(1)), "C1"),
        ]);
        let rows: Vec<_> = list.iter().map(|d| (d.kind, d.id.as_str())).collect();
        assert_eq!(
            rows,
            [(DeviceKind::Keyboard, "kb1"), (DeviceKind::Mouse, "mouse")]
        );
    }

    #[test]
    fn different_containers_are_not_merged() {
        let list = build_list(vec![
            in_container(raw("a", Some("Board"), RawKind::Keyboard, None), "C1"),
            in_container(raw("b", Some("Board"), RawKind::Keyboard, None), "C2"),
        ]);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn a_folded_row_takes_a_name_from_its_siblings() {
        let list = build_list(vec![
            in_container(raw("a", None, RawKind::Keyboard, None), "C1"),
            in_container(raw("b", Some("Board"), RawKind::Keyboard, None), "C1"),
        ]);
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name.as_deref(), Some("Board"));
    }

    #[test]
    fn the_representative_does_not_depend_on_listing_order() {
        let a = in_container(raw("a", Some("Board"), RawKind::Keyboard, None), "C1");
        let b = in_container(raw("b", Some("Board"), RawKind::Keyboard, None), "C1");
        let forward = build_list(vec![a.clone(), b.clone()]);
        let backward = build_list(vec![b, a]);
        assert_eq!(forward, backward);
        assert_eq!(forward[0].id, "a");
    }

    #[test]
    fn order_is_by_kind_then_name_then_id() {
        let list = build_list(vec![
            raw("3", Some("zeta"), RawKind::Mouse, None),
            raw("2", Some("Beta"), RawKind::Keyboard, None),
            raw("1", Some("alpha"), RawKind::Keyboard, None),
            raw("5", None, RawKind::Keyboard, None),
            raw("4", Some("alpha"), RawKind::Keyboard, None),
        ]);
        let ids: Vec<_> = list.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["5", "1", "4", "2", "3"]);
    }

    #[test]
    fn ignored_entries_are_dropped() {
        let list = build_list(vec![raw("c", Some("Consumer"), hid(0x0C, 0x01), Some(1))]);
        assert!(list.is_empty());
    }
}
