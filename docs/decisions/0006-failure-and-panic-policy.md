# ADR 0006 - Failure and panic policy

- Status: Accepted, 2026-10-01
- Deciders: Max
- Related: invariants 1, 6, 10, 13; §19, §47; `docs/research/phase-0-windows-input.md` (B9);
  `crates/keyclean-win/src/{hook,watchdog,engine}.rs`, `crates/keyclean-core/src/policy.rs`

## Context

Every failure must end with input released. Rust offers two panic strategies: unwinding (the stack is
unwound and the program may continue) and aborting (the process ends at once). A panic that tries to
unwind out of the hook callback, an `extern "system"` function, aborts the process anyway since Rust
1.81. And an engine thread that hangs while a hook is installed stalls input until Windows removes the
hook.

## Decision

1. **`panic = "abort"` in both `[profile.dev]` and `[profile.release]`.** Any panic anywhere ends the
   process, and Windows removes the hook of a dead process. There is no half-alive state.
2. **`catch_unwind` at the hook FFI boundary** stays as a defence: if a build ever unwinds, a panic in
   the decision code passes the event instead of crossing into Windows (invariant 10).
3. **Watchdog escalation.** A separate thread waits (no polling) for the hard deadline. When it passes,
   the watchdog switches the hook to Passthrough so every event passes, posts `WM_WATCHDOG_EXPIRED` to
   the engine, and if the engine doesn't acknowledge within **1 s**, calls `std::process::abort()`.
4. **The hard deadline is enforced twice** (invariant 6): the hook compares QPC ticks with an atomic
   deadline on every event, and the watchdog fires on its own. Hard deadline =
   `min(max_lock, session + 10 s)`.
5. **Absolute 60-minute ceiling on the max-lock setting** (`ABSOLUTE_MAX_LOCK`). The spec only sets a
   30-minute default (§19, §32). We add a hard ceiling so a corrupt or hostile setting can't produce a
   multi-hour hard deadline. This is stricter than the spec, which is why it is recorded here.
6. **Debug dev cap** (invariant 13): `keyclean-win` itself forces the Dev profile under
   `cfg!(debug_assertions)` (session ≤ 15 s, hard deadline ≤ 20 s, "DEV CAP"), whatever the caller asks.
7. **Generation numbers on posted messages.** Each session gets a generation number; hook and watchdog
   messages carry it in `wParam`, and the engine ignores messages from an earlier session. A late
   "chord" or "deadline" message can't end a newer lock.
8. **No `unwrap`/`expect` on engine paths.** Errors end the session with `EngineError` and input
   released; illegal state transitions return `Err`, never panic.

In plain terms: if anything goes badly wrong, KeyClean would rather quit than keep the keyboard locked.
Quitting is the one release that can't fail, because Windows cleans up after a dead process.

## Alternatives

- **Unwinding with recovery** - keeps the app alive after a panic but risks running in an inconsistent
  state with a hook installed. Rejected.
- **Watchdog that only logs or notifies** - doesn't release input if the engine is stuck. Rejected.
- **Watchdog that unhooks by itself** - `UnhookWindowsHookEx` is meant to be called by the installing
  thread; switching to Passthrough plus process abort works from any thread. [assumption]
- **No absolute ceiling** - follows the spec literally but trusts the settings file. Rejected.

## Consequences

- A panic shows no in-app error; the app just disappears. Local crash reporting can come later.
- The watchdog can kill the whole app, including the UI, in the rare engine-hang case. Acceptable.
- Release builds never allow a lock longer than 60 minutes.
