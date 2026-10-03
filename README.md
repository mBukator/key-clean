# KeyClean

A lightweight Windows utility for safely locking
keyboard and pointing-device input while cleaning
or testing your devices.

## Status

Pre-alpha. KeyClean is in Milestone 1 (keyboard lock engine). There are no releases yet, and nothing here is ready for everyday use. See [docs/ROADMAP.md](docs/ROADMAP.md) for progress.

## Safety

KeyClean always hands control back. Every lock has four independent exits:

1. **Session timer** - the lock ends when the countdown reaches zero.
2. **Hard deadline** - a separate safety limit releases input even if the timer fails.
3. **Ctrl+Alt+K** - the emergency chord unlocks immediately.
4. **Process exit** - KeyClean uses only user-mode hooks, so Windows releases input if the app closes or crashes.

KeyClean never disables devices, installs drivers, or writes persistent lock state.

## Privacy

KeyClean does not record or transmit the contents of keyboard input. During a lock session, selected input is blocked rather than recorded. There is no telemetry, no account, and no network access.

## Development

Prerequisites:

- Windows 10 or 11 (x64)
- [Rust](https://rustup.rs/) stable, MSVC toolchain
- Visual Studio Build Tools with the "Desktop development with C++" workload and a Windows SDK
- [bun](https://bun.sh/)
- Microsoft Edge WebView2 Runtime (preinstalled on Windows 11)

Commands:

```sh
bun install              # install frontend dependencies and git hooks
bun tauri dev            # run the app in development mode
cargo test --workspace   # run Rust unit tests
```

Read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request.

## License

[Apache-2.0](LICENSE)
