# Contributing to KeyClean

Thanks for your interest in KeyClean.

## Before you start

- Read the **Safety invariants** and **Privacy invariants** in [CLAUDE.md](CLAUDE.md). Every change must keep them intact. A change that needs to relax one requires an approved ADR in `docs/decisions/`.
- Check the product rule (spec §65): _does this make KeyClean better at safely controlling, cleaning, or diagnosing input devices on Windows?_ If not, it does not belong in KeyClean.
- Follow the branch, commit, and pull request rules in [docs/development/git-workflow.md](docs/development/git-workflow.md).

## Gate

Run this before opening a pull request. CI runs the same checks.

```sh
cargo fmt --all --check && bun run build && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && bun run typecheck && bun run lint && bun run format:check
```

## Testing input locking

Changes that affect OS behavior are verified with the manual test scripts in `docs/testing/manual/`. Update the relevant script when behavior changes.

## Privacy

Never include typed text, key codes, or scan codes in issues, logs, tests, or screenshots.
