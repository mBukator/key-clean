//! Private window messages and timer ids of the engine's hidden window.

use windows::Win32::UI::WindowsAndMessaging::WM_APP;

/// Posted by the hook: the emergency chord completed (phase is already `Draining`).
pub(crate) const WM_HOOK_CHORD: u32 = WM_APP + 1;
/// Posted by the hook: it saw the hard deadline pass (phase is already `Draining`).
pub(crate) const WM_HOOK_DEADLINE: u32 = WM_APP + 2;
/// Posted by the hook: every blocked press has been released during `Draining`.
pub(crate) const WM_HOOK_DRAINED: u32 = WM_APP + 3;
/// Posted by `Engine` methods: commands are waiting in the channel.
pub(crate) const WM_COMMANDS_READY: u32 = WM_APP + 10;
/// Posted by the watchdog: the hard deadline passed (phase is already `Passthrough`).
pub(crate) const WM_WATCHDOG_EXPIRED: u32 = WM_APP + 11;

/// `SetTimer` id for the session duration.
pub(crate) const TIMER_SESSION: usize = 1;
/// `SetTimer` id for the drain timeout.
pub(crate) const TIMER_DRAIN: usize = 2;
