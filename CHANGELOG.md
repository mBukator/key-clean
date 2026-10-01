# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Project scaffold: Rust workspace (`keyclean-core`, `keyclean-win`, Tauri app shell), React + TypeScript + Tailwind UI, shared English strings, commit tooling, CI, and repository docs.
- Keyboard lock engine (M1): a user-mode low-level keyboard hook on a dedicated engine thread that blocks every key during a lock and leaves no stuck keys afterwards.
- Emergency unlock with Ctrl+Alt+K (either Ctrl, either Alt, AltGr counts), detected inside the hook.
- Hard deadline enforced twice, by the hook on every event and by a separate watchdog thread that ends the process if the engine stops responding.
- Debug dev cap: development builds lock for at most 15 s with a 20 s hard deadline and show "DEV CAP".
- Automatic unlock on system transitions: sleep, shutdown, restart, logoff, workstation lock and session switch.
- Keyboard list (name and id only) from Raw Input.
- Bare Tauri shell with a Lock button, status line and keyboard list.
- Documentation: Phase 0 research notes, fail-safe matrix, ADRs 0001-0007, roadmap, and the M1 manual test.
