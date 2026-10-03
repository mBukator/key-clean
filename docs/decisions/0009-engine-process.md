# ADR 0009 - Run the engine in its own process

- Status: Accepted, 2026-10-03
- Deciders: Max (decision 2026-10-02)
- Related: `crates/keyclean-win/src/{host,client,protocol}.rs`, `src-tauri/src/main.rs`,
  `docs/research/webview-focus-hook.md`, fail-safe matrix, M1.md steps F1/F3, harness check S14.
  Amends the project guide's "Engine model" (one engine thread in the app process).

## Context

In M1 the engine thread, and with it the `WH_KEYBOARD_LL` hook, ran inside the Tauri app process.
Max's manual tests found that while the app's own WebView2 window has focus, Windows intermittently
stops calling that hook:

- F1: with the KeyClean window focused, Win opened the Start menu during a lock.
- F3: with the KeyClean window focused, Ctrl+Alt+K needed several tries.
- F2, F4: with Notepad focused, the same keys were blocked and the chord worked first time.

The same experiments with the hook in another process (`lock_smoke`, experiments X1/X2) worked
perfectly with the KeyClean window focused, and with VS Code or Chrome focused. So the trigger is
specific: **the hook's own process owns the focused WebView2 window** [tested]. Others have reported
the same behaviour (tauri-apps/tauri#13919,
[wh-keyboard-ll-chromium](https://github.com/wudaming00/wh-keyboard-ll-chromium)) without a known
root cause, and installing a second hook can mask it. Harness check S14 reproduces it without a
second hook: it focuses the app window with a synthesized click, sends Ctrl+Alt+K, and on the
in-process engine the session ended by `UserRequest` (at app close), not `Emergency`.

This breaks invariant 1 (the emergency chord must always work) whenever the user has clicked into
KeyClean's own window, which is exactly what they do to start a lock. From M6 on, the overlay is a
WebView2 window that has focus for the entire lock, so the problem would get worse.

## Decision

Run the engine in a separate process with no WebView: the same executable, started by the app as
`keyclean.exe --engine --parent <app pid>`.

- **Dispatch.** `main.rs` checks for `--engine` before anything touches Tauri, so the engine process
  never creates a webview or registers the single-instance plugin. `keyclean_win::host::run` runs the
  existing `Engine` unchanged.
- **Protocol.** Line-delimited JSON (`protocol.rs`): commands (`lock`, `unlock`, `shutdown`) on the
  engine's stdin, events (`ready`, `status`, `ended`, `error`, `notice`) on its stdout. Session
  events only; never key data (privacy invariants). Errors cross as their message key and technical
  details (`EngineError::Remote`).
- **Client.** `keyclean_win::client::EngineClient` has the same API as `Engine`. It starts the process
  with piped stdin/stdout and `CREATE_NO_WINDOW`, waits up to 5 s for `ready`, and turns event lines
  back into `EngineEvent`s on a reader thread. Keyboard enumeration stays in the app process (it
  installs no hook).
- **Shutdown.** Dropping the client sends `shutdown`, closes stdin, waits up to 5 s for the process to
  exit (longer than the engine's own 3 s shutdown wait), then kills it.
- **The engine process ends when the app does,** by whichever comes first:
    - stdin reaches EOF (the app closed it or died): the engine is dropped, which releases any lock;
    - a watcher thread waiting on the app process (`OpenProcess(SYNCHRONIZE)` +
      `WaitForSingleObject`) sees it exit: the process exits. This covers a stdin pipe kept open by a
      handle another process inherited. If the app can't be opened, the watcher logs it and the
      engine relies on EOF;
    - a write to stdout fails: the process exits.

    Each of these ends the process, and Windows removes the hook of a process that exits, so input is
    released.

- **Unexpected engine exit.** The client reports `EngineError::EngineProcess` (message key
  `error.engine_stopped`), plus `SessionEnded { EngineError }` if a session was active (input is
  already released, because the hook died with the process), and the UI marks the engine unavailable.
  There is no automatic restart in M1.
- `lock_smoke` and the harness's engine checks keep using the in-process `Engine` directly; they have
  no WebView.
- New dependencies in `keyclean-win`: `serde` (derive) and `serde_json`, for the wire protocol. Both
  were already in the build through Tauri.

## How the invariants read now

- **Invariant 1 (four exits).** Unchanged inside the engine process. "OS cleanup when the process
  dies" now covers two processes: if the engine process dies, Windows removes its hook; if the app
  dies, the engine process follows (EOF or parent watch) and Windows removes the hook.
- **Invariant 3 (engine owns lock state).** Stronger than before: a hung, crashed or closed webview
  can't even share an address space with the hook. The engine never waits on the app: events go into
  an unbounded channel, and a separate forwarder thread writes them to stdout.
- **Invariant 4 (no hook at idle).** Unchanged: the engine process at idle has its hidden window and
  its power/session notifications, but no hook, no Raw Input registration and no polling. It is one
  extra idle process (a few MB).
- **Invariant 12 (single instance).** One KeyClean instance is now two `keyclean.exe` processes: the
  app and its engine. The single-instance plugin still allows only one app, and only the app starts an
  engine.

## Alternatives

- **Keep the hook in the app process and work around the focus bug** (a second "keep-alive" hook,
  re-installing the hook, a different WebView2 configuration). Rejected: no root cause is known, the
  reported masking by a second hook is not understood, and a workaround we can't explain is not a
  safety mechanism.
- **A separate engine executable.** Same isolation, but a second binary to build, bundle and sign, and
  version skew between the two. The same exe with a flag avoids both.
- **Named pipes or a socket instead of stdio.** More setup and a name other processes could find.
  Anonymous stdio pipes are private to the two processes and close by themselves when either exits.
- **A Windows job object with kill-on-close** so the engine dies with the app. Adds `unsafe` and only
  covers the app-dies case, which EOF and the parent watcher already cover.

## Consequences

- The focus bug can't reach the hook: S14 is expected to pass, and F1/F3 are re-checked by hand.
- One more process at runtime. Task Manager shows two `keyclean.exe` entries while the app runs.
  `Stop-Process -Name keyclean -Force` ends both.
- About 5 s of extra failure surface at startup (spawn and handshake). If the engine process can't
  start, the app shows "KeyClean's input engine has stopped" with technical details.
- In debug builds the engine process writes its diagnostics to the app's stderr (so `bun tauri dev`
  and the harness logs show them); release builds discard them.
- The harness checks S10 and S11 now also assert that no `keyclean.exe` is left within 5 s of the app
  exiting.
- Open: if the engine process dies unexpectedly, the app stays up with locking disabled until it is
  restarted. An automatic restart can come later (M3 crash recovery).
