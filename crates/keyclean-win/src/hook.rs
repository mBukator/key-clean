//! The `WH_KEYBOARD_LL` hook.
//!
//! The callback runs on the engine thread (the thread that installed it) whenever that thread
//! waits for messages. It must return quickly: Windows silently removes a low-level hook that
//! exceeds `LowLevelHooksTimeout` (at most 1 s since Windows 10 1709), which makes the lock fail
//! open. So the callback only reads atomics and a thread-local `Copy` value, does fixed-size bit
//! math, and at most posts a message to its own window. No I/O, logging, allocation, locks or
//! cross-thread waits (invariant 5).
//!
//! Shared state:
//! - [`PHASE`], [`HARD_DEADLINE_TICKS`], [`ENGINE_HWND`]: atomics, because the watchdog thread
//!   writes `PHASE` too.
//! - [`LOCAL`]: the key tracker and chord state. Only the engine thread touches it (the callback
//!   and the engine's own code), so a thread-local `Cell` is enough.
//!
//! Nothing that identifies a key leaves this module. Key codes are used for the pass/block
//! decision and then forgotten.

use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU8, AtomicU64, Ordering};

use keyclean_core::chord::{ChordState, KeyDirection};
use keyclean_core::keystate::{KeyTracker, Phase, Verdict};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::GetAsyncKeyState;
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, HC_ACTION, HHOOK, KBDLLHOOKSTRUCT, LLKHF_EXTENDED, LLKHF_UP, PostMessageW,
    SetWindowsHookExW, UnhookWindowsHookEx, WH_KEYBOARD_LL,
};

use crate::msg::{WM_HOOK_CHORD, WM_HOOK_DEADLINE, WM_HOOK_DRAINED};
use crate::{keys, qpc};

/// `PHASE` value while no hook is installed. Decodes as `Passthrough`, the safe direction.
const PHASE_IDLE: u8 = 0;

static PHASE: AtomicU8 = AtomicU8::new(PHASE_IDLE);
static HARD_DEADLINE_TICKS: AtomicU64 = AtomicU64::new(u64::MAX);
static ENGINE_HWND: AtomicIsize = AtomicIsize::new(0);
static END_POSTED: AtomicBool = AtomicBool::new(false);
static DRAIN_POSTED: AtomicBool = AtomicBool::new(false);
/// Generation of the current session, sent with every hook message so the engine can ignore
/// messages left over from an earlier session.
static SESSION_GENERATION: AtomicU64 = AtomicU64::new(0);
/// QPC ticks of the last event blocked while draining (a blocked key still auto-repeating or
/// being released). The engine extends the drain while this keeps moving.
static LAST_DRAIN_BLOCK_TICKS: AtomicU64 = AtomicU64::new(0);
/// QPC ticks of the last `HC_ACTION` call of this session (0 = none yet). The engine compares it
/// with keyboard Raw Input to notice a hook Windows removed or skipped (hook-liveness check).
static LAST_CALLBACK_TICKS: AtomicU64 = AtomicU64::new(0);
/// Testkit only: milliseconds the next callback sleeps, to provoke a real `LowLevelHooksTimeout`.
#[cfg(feature = "testkit")]
pub(crate) static STALL_NEXT_CALLBACK_MS: std::sync::atomic::AtomicU32 =
    std::sync::atomic::AtomicU32::new(0);

#[derive(Clone, Copy)]
struct HookLocal {
    tracker: KeyTracker,
    chord: ChordState,
}

impl HookLocal {
    const fn new() -> Self {
        HookLocal {
            tracker: KeyTracker::new(),
            chord: ChordState::new(),
        }
    }
}

thread_local! {
    static LOCAL: Cell<HookLocal> = const { Cell::new(HookLocal::new()) };
}

/// The hook procedure registered with `SetWindowsHookExW`.
///
/// # Safety
/// Called by Windows only, with the arguments documented for `LowLevelKeyboardProc`.
unsafe extern "system" fn keyboard_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 && lparam.0 != 0 {
        LAST_CALLBACK_TICKS.store(qpc::ticks(), Ordering::Release);
        // Testkit only, never compiled into the app: a deliberately slow callback.
        #[cfg(feature = "testkit")]
        {
            let stall = STALL_NEXT_CALLBACK_MS.swap(0, Ordering::AcqRel);
            if stall != 0 {
                std::thread::sleep(std::time::Duration::from_millis(u64::from(stall)));
            }
        }
        // SAFETY: for WH_KEYBOARD_LL with HC_ACTION, `lparam` points to a KBDLLHOOKSTRUCT that is
        // valid for the duration of this call (LowLevelKeyboardProc docs). We copy the fields out.
        let info = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
        let (vk, scan, flags) = (info.vkCode, info.scanCode, info.flags.0);
        // No panic may unwind across this FFI boundary (invariant 10). With `panic = "abort"` a
        // panic ends the process (and Windows removes the hook); in unwind builds, pass the event.
        let verdict =
            std::panic::catch_unwind(|| on_key_event(vk, scan, flags)).unwrap_or(Verdict::Pass);
        if verdict == Verdict::Block {
            return LRESULT(1);
        }
    }
    // SAFETY: forwarding the unmodified arguments to the next hook, as the docs require.
    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn on_key_event(vk: u32, scan: u32, flags: u32) -> Verdict {
    let direction = if flags & LLKHF_UP.0 != 0 {
        KeyDirection::Up
    } else {
        KeyDirection::Down
    };
    let extended = flags & LLKHF_EXTENDED.0 != 0;
    let code = (vk & 0xFF) as u8;

    let mut phase = Phase::from_u8(PHASE.load(Ordering::Acquire));

    // Hard deadline, enforced on every event (invariant 6). Before it, a lock in progress moves
    // to Draining; past it, a drain still running moves to Passthrough, so the hook alone releases
    // everything even if the watchdog has failed.
    if matches!(phase, Phase::Arming | Phase::Locked | Phase::Draining)
        && qpc::ticks() >= HARD_DEADLINE_TICKS.load(Ordering::Acquire)
    {
        if phase == Phase::Draining {
            if transition(Phase::Draining, Phase::Passthrough) {
                post_once(&DRAIN_POSTED, WM_HOOK_DRAINED);
            }
        } else if transition(phase, Phase::Draining) {
            post_once(&END_POSTED, WM_HOOK_DEADLINE);
        }
        phase = Phase::from_u8(PHASE.load(Ordering::Acquire));
    }

    LOCAL
        .try_with(|cell| {
            let mut local = cell.get();
            let fired = local
                .chord
                .on_key(keys::chord_key(vk, scan, extended), direction);
            if fired && phase == Phase::Locked && transition(Phase::Locked, Phase::Draining) {
                post_once(&END_POSTED, WM_HOOK_CHORD);
            }
            // The completing chord key is decided under the phase it arrived in, so it is
            // swallowed and tracked like any other blocked press.
            let verdict = local.tracker.decide(phase, code, direction);
            if verdict == Verdict::Block && phase == Phase::Draining {
                LAST_DRAIN_BLOCK_TICKS.store(qpc::ticks(), Ordering::Release);
            }
            if local.tracker.drained()
                && Phase::from_u8(PHASE.load(Ordering::Acquire)) == Phase::Draining
            {
                post_once(&DRAIN_POSTED, WM_HOOK_DRAINED);
            }
            cell.set(local);
            verdict
        })
        .unwrap_or(Verdict::Pass)
}

fn transition(from: Phase, to: Phase) -> bool {
    PHASE
        .compare_exchange(from as u8, to as u8, Ordering::AcqRel, Ordering::Acquire)
        .is_ok()
}

fn post_once(flag: &AtomicBool, msg: u32) {
    if !flag.swap(true, Ordering::AcqRel) {
        post(msg, SESSION_GENERATION.load(Ordering::Acquire));
    }
}

/// Posts `msg` to the engine window with `generation` in `wParam`. Never blocks.
pub(crate) fn post(msg: u32, generation: u64) {
    let hwnd = ENGINE_HWND.load(Ordering::Acquire);
    if hwnd != 0 {
        // SAFETY: PostMessageW only enqueues; a stale or invalid HWND makes it fail, not misbehave.
        // If it fails, the engine's timers and the watchdog still end the session.
        let _ = unsafe {
            PostMessageW(
                Some(HWND(hwnd as *mut _)),
                msg,
                WPARAM(generation as usize),
                LPARAM(0),
            )
        };
    }
}

/// Records the engine window, so the hook and the watchdog can post to it.
pub(crate) fn set_engine_window(hwnd: HWND) {
    ENGINE_HWND.store(hwnd.0 as isize, Ordering::Release);
}

/// Prepares the shared state for a new lock. Engine thread only, before [`install`].
pub(crate) fn begin_arming(generation: u64, hard_deadline_ticks: u64) {
    let _ = LOCAL.try_with(|cell| cell.set(HookLocal::new()));
    SESSION_GENERATION.store(generation, Ordering::Release);
    HARD_DEADLINE_TICKS.store(hard_deadline_ticks, Ordering::Release);
    END_POSTED.store(false, Ordering::Release);
    DRAIN_POSTED.store(false, Ordering::Release);
    LAST_CALLBACK_TICKS.store(0, Ordering::Release);
    PHASE.store(Phase::Arming as u8, Ordering::Release);
}

/// Installs the hook. Engine thread only; the thread must run a message loop.
pub(crate) fn install() -> windows::core::Result<HHOOK> {
    // SAFETY: GetModuleHandleW(None) returns this executable's module handle without
    // transferring ownership.
    let module = unsafe { GetModuleHandleW(None) }?;
    // SAFETY: `keyboard_proc` matches HOOKPROC and lives for the whole program; a global
    // low-level hook needs the module handle and thread id 0 (SetWindowsHookExW docs).
    unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), Some(module.into()), 0) }
}

/// Adds the keys Windows currently sees as held to the tracker, and seeds the chord detector.
/// Engine thread only, between [`install`] and switching to `Locked`. This runs outside the hook
/// callback, where `GetAsyncKeyState` is still accurate because nothing has been blocked yet.
pub(crate) fn seed_from_held_keys() {
    let _ = LOCAL.try_with(|cell| {
        let mut local = cell.get();
        for vk in 1..=u8::MAX {
            if is_held(u32::from(vk)) {
                local.tracker.os_down.insert(vk);
            }
        }
        for (vk, key) in keys::CHORD_SEED_KEYS {
            if is_held(vk) {
                local.chord = local.chord.with_held(key);
            }
        }
        cell.set(local);
    });
}

fn is_held(vk: u32) -> bool {
    // SAFETY: GetAsyncKeyState has no preconditions; the high bit means "currently down".
    let state = unsafe { GetAsyncKeyState(vk as i32) };
    state < 0
}

/// Switches from `Arming` to `Locked`. Returns false if something else changed the phase first
/// (for example the hard deadline passed during arming).
pub(crate) fn engage() -> bool {
    transition(Phase::Arming, Phase::Locked)
}

/// Moves `Arming`/`Locked` to `Draining`. Leaves `Draining` and `Passthrough` as they are.
pub(crate) fn enter_draining() {
    let _ =
        transition(Phase::Arming, Phase::Draining) || transition(Phase::Locked, Phase::Draining);
}

/// Watchdog override: pass everything, unless no hook is active.
pub(crate) fn force_passthrough() {
    // A compare-exchange loop rather than `fetch_update`, which newer Rust deprecates in favour of
    // `try_update`, which older toolchains lack.
    let mut current = PHASE.load(Ordering::Acquire);
    while current != PHASE_IDLE {
        match PHASE.compare_exchange_weak(
            current,
            Phase::Passthrough as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => return,
            Err(actual) => current = actual,
        }
    }
}

/// The current phase, or `None` when no hook is active.
pub(crate) fn phase() -> Option<Phase> {
    match PHASE.load(Ordering::Acquire) {
        PHASE_IDLE => None,
        p => Some(Phase::from_u8(p)),
    }
}

/// Marks the start of a drain as the last drain activity.
pub(crate) fn mark_drain_start(ticks: u64) {
    LAST_DRAIN_BLOCK_TICKS.store(ticks, Ordering::Release);
}

/// QPC ticks of the last event blocked while draining (or of the drain start).
pub(crate) fn last_drain_block_ticks() -> u64 {
    LAST_DRAIN_BLOCK_TICKS.load(Ordering::Acquire)
}

/// QPC ticks of the last hook call this session, or `None` if the hook hasn't been called.
pub(crate) fn last_callback_ticks() -> Option<u64> {
    match LAST_CALLBACK_TICKS.load(Ordering::Acquire) {
        0 => None,
        t => Some(t),
    }
}

/// Whether every blocked press has been released. Engine thread only.
pub(crate) fn drained() -> bool {
    LOCAL
        .try_with(|cell| cell.get().tracker.drained())
        .unwrap_or(true)
}

/// Removes the hook and returns the shared state to idle. Engine thread only.
///
/// Returns `Err` if Windows had already removed the hook (e.g. it timed out); input is released
/// either way.
pub(crate) fn uninstall(hook: HHOOK) -> windows::core::Result<()> {
    PHASE.store(Phase::Passthrough as u8, Ordering::Release);
    // SAFETY: `hook` came from `install` on this thread and is unhooked at most once (the caller
    // takes it out of its Option first).
    let result = unsafe { UnhookWindowsHookEx(hook) };
    clear();
    result
}

/// Marks the hook state idle without unhooking (used when installation failed).
pub(crate) fn clear() {
    PHASE.store(PHASE_IDLE, Ordering::Release);
    HARD_DEADLINE_TICKS.store(u64::MAX, Ordering::Release);
    LAST_CALLBACK_TICKS.store(0, Ordering::Release);
    let _ = LOCAL.try_with(|cell| cell.set(HookLocal::new()));
}
