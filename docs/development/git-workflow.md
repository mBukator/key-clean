# Git workflow

These rules apply to every contributor, human or automated. Every rule is a MUST. Hooks and CI enforce most of them.

## 1. Conventional Commits

Every commit header has the form:

```text
<type>(<scope>): <subject>
```

Allowed types: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.

## 2. Scope is required

The scope must be one of:

| Scope    | Covers                                                        |
| -------- | ------------------------------------------------------------- |
| `engine` | `crates/keyclean-win`: hooks, engine thread, watchdog, Win32  |
| `core`   | `crates/keyclean-core`: state machine, timers, safety policy  |
| `app`    | `src-tauri`: app shell, commands, tray, windows, plugins      |
| `ui`     | `src/`: React UI                                              |
| `i18n`   | `locales/` and the localization helpers                       |
| `docs`   | `docs/` and repository docs                                   |
| `ci`     | `.github/` workflows and templates                            |
| `repo`   | root tooling and config (package.json, linters, hooks, Cargo) |
| `deps`   | dependency additions, removals, and upgrades                  |

A scope is at most 20 characters.

## 3. Subject

- Imperative mood: "add", not "added" or "adds".
- Lowercase first letter.
- No trailing period.
- The whole header is at most 72 characters.

## 4. Body is mandatory

Every commit has a body. No exceptions; commitlint rejects commits without one.

- Leave one blank line after the header.
- Wrap at about 72 characters.
- Explain what changed and why, not how.
- Use bullets when the commit makes several changes.

## 5. Footers

- `BREAKING CHANGE: <description>` for breaking changes.
- `Closes #N` to link an issue.
- A commit that adds a dependency explains why in one line in the body.

## 6. No AI attribution

Never mention Claude or AI in commits or pull requests. No `Co-Authored-By` trailer.

## 7. Punctuation

Use hyphens. Never use em dashes or en dashes in commits or pull requests.

### Examples

Good:

```text
feat(engine): install keyboard hook only during a session

The low-level keyboard hook was installed at startup, which meant
KeyClean saw input while idle. Install it when a session starts and
remove it when the session ends.

Closes #12
```

```text
build(deps): add windows crate for Win32 bindings

- add windows 0.62 with the UI input and threading features
- needed for SetWindowsHookExW and the engine message loop
```

Bad:

```text
Added keyboard hook.
```

No type, no scope, past tense, trailing period, no body.

```text
feat: add hook
```

Missing scope and body.

```text
fix(engine): Fix stuck keys; release modifiers on unlock.
```

Uppercase subject, trailing period, no body. Separate clauses with a hyphen or semicolon, never an em dash.

```text
feat(keyboard-lock-engine): add hook
```

Scope is not in the allowed list.

## 8. Branches

- `main` holds tagged releases. `develop` is the integration branch. Both are protected; nobody commits to them directly.
- Work branches start from `develop`: `feat/*`, `fix/*`, `docs/*`, `chore/*`, `refactor/*`. Use descriptive names, for example `feat/m1-keyboard-lock`.
- `hotfix/*` branches start from `main`.

Good: `feat/m1-keyboard-lock`, `fix/stuck-alt-after-unlock`, `docs/manual-test-m1`.

Bad: `my-branch`, `fix`, `wip2`, `feature/stuff`.

## 9. Pull requests

- Target `develop`.
- Fill in the pull request template completely.
- The title is a Conventional Commit header; it becomes the squash commit.
- Squash and merge only, with CI green.
- Delete the branch after merge.
- Issues use the issue templates.

## 10. Automation and agents

Automation and agents may push work branches and open pull requests. They must never:

- push to `main` or `develop`
- merge a pull request
- force-push
- rewrite history (rebase, amend, or reset published commits)
- use `--no-verify`; if a hook is broken, fix the hook

## 11. Git hooks

Husky installs these on `bun install`:

| Hook         | Runs                                                               |
| ------------ | ------------------------------------------------------------------ |
| `commit-msg` | `commitlint`                                                       |
| `pre-commit` | `lint-staged`, then `bun run typecheck`                            |
| `pre-push`   | `bun run build`, `cargo clippy`, `cargo test`, then `bun run lint` |

## 12. Writing commits

Author commits interactively with `bunx merlin` (it reads the commitlint config), or with `git commit` following the same rules.

## 13. Pre-PR gate

Run this before opening a pull request:

```sh
cargo fmt --all --check && bun run build && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && bun run typecheck && bun run lint && bun run format:check
```
