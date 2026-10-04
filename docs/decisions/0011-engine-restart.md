# ADR 0011 - Restart a crashed engine process, a limited number of times

- Status: Accepted, 2026-10-04
- Deciders: Max (2026-10-04)
- Related: `crates/keyclean-core/src/restart.rs`, `src-tauri/src/lib.rs`, ADR 0009, fail-safe matrix,
  harness check S21, M3 step 8.

## Context

Since ADR 0009 the lock engine runs as its own process. If that process dies, Windows removes its
hook, so input comes back at once. Until M3, though, the app then disabled locking until it was
restarted by hand. §53 asks for crash recovery.

## Decision

- When the engine process ends without the app asking, the app starts a new one.
- The new engine is **idle**. A lock is never resumed or restarted on its own. The user sees "KeyClean's
  lock engine stopped and was restarted. Nothing is locked."
- **The limit:** at most 3 restarts in a sliding 5-minute window (`keyclean_core::restart::RestartBudget`,
  unit-tested with a fake clock). Past it, the app keeps today's behaviour: the engine is unavailable
  and the error is shown.
- No restart while the app is exiting. A flag set before the engine is stopped makes sure of that.

## Alternatives

- **Stay stopped (M1 behaviour).** Rejected: the user had to restart the app to clean again.
- **Show a "Restart engine" button.** Rejected: it asks a question the user can't usefully answer,
  and the safe answer is always yes.
- **Restart without a limit.** Rejected: an engine that crashes at start would spin.

## Consequences

- A crash costs the current lock (input is already released) but not the session of using the app.
- Repeated crashes stop after three, so they show up as an error instead of a loop.
