<!--
Title format: <type>(<scope>): <subject>
Types: feat, fix, docs, style, refactor, perf, test, build, ci, chore, revert
Scopes: engine, core, app, ui, i18n, docs, ci, repo, deps
The title becomes the squash commit, so keep it under 72 characters, imperative, no trailing period.
-->

## Overview

<!-- What does this PR do and why? One or two sentences. -->

## Changes

-

## Technical details

**Files added:**

-

**Files modified:**

-

## Testing

Gate command (must pass):

```sh
cargo fmt --all --check && bun run build && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && bun run typecheck && bun run lint && bun run format:check
```

- [ ] Gate passes locally

## Manual test

<!-- Which docs/testing/manual/<milestone>.md steps did you run, and what happened? Write "n/a" if the change does not touch OS behavior. -->

| Step | Result |
| ---- | ------ |
|      |        |

## Related issues

Closes #

## Checklist

- [ ] Title and commits follow Conventional Commits with a valid scope and a body
- [ ] The gate command passes
- [ ] Safety invariants in CLAUDE.md are untouched, or the change is covered by an approved ADR
- [ ] No keystroke data (characters, VK codes, scan codes) in logs, errors or events
- [ ] User-facing strings live in `locales/en/strings.json`
- [ ] Manual test file updated if OS behavior changed
- [ ] ROADMAP and CHANGELOG updated when a milestone changes
