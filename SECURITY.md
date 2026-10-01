# Security policy

## Reporting a vulnerability

Please report security issues privately through [GitHub security advisories](https://github.com/mBukator/key-clean/security/advisories/new). Do not open a public issue.

## Scope

In scope:

- Anything that could leave keyboard or pointing-device input locked, including failures of the timer, hard deadline, emergency chord, or OS cleanup on process exit.
- Anything that could record, store, leak, or transmit keystrokes or other input contents.
- Anything that leaves persistent system changes after KeyClean exits.

## Supported versions

KeyClean is pre-release. Only the latest code on `main` is supported.
