# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

The project's rules, architecture, commands, and contract-test map live in
`AGENTS.md`, which is shared with Codex and other agents. It is imported here and is binding:

@AGENTS.md

## Claude Code specifics

- **Slash commands.** Every folder in `.claude/skills/` is a `/<name>` command.
  `/pr-opportunity-scan`, `/ollama-doctor`, and `/docs-sync` are manual only
  (`disable-model-invocation: true`); the other three can also trigger from their
  descriptions. Pair `/check-security-invariants` with the built-in
  `/security-review` on any networking, auth, or CI change.
- **Permissions.** `.claude/settings.json` pre-approves `./scripts/ci.sh`, the
  underlying `cargo fmt`/`clippy`/`test`/`check`/`build`/`fetch`/`tree`/`deny`
  commands, and read-only git. It denies reading `.env` files. Put personal
  additions in `.claude/settings.local.json` (gitignored).
- **Web sessions.** `.claude/hooks/session-start.sh` runs only when
  `CLAUDE_CODE_REMOTE=true`. It installs the pinned 1.88 toolchain before the
  session starts, then warms the crate cache and test build in the background
  (log: `$CARGO_WARM_LOG`). A first `cargo` command that prints
  `Blocking waiting for file lock` is waiting on that warm-up, not hanging.
- **Linux vs macOS.** Web sessions run Linux, so `app.rs` is not compiled; say so
  whenever you report a green local gate for an AppKit change.
- **Editing these files.** Shared facts go in `AGENTS.md`; only Claude-specific
  wiring belongs here (`/docs-sync` keeps both current).
- **Verify before asserting.** Treat doc claims (versions, routes, test names) as
  hypotheses until checked against `src/` or `tests/`, and separate "verified by a
  command I ran" from "inferred".
