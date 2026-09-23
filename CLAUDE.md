# CLAUDE.md

Guidance for Claude Code (claude.ai/code) in this repository. The project's rules,
architecture, commands, and contract-test map live in `AGENTS.md`, which is shared
with Codex and other agents. It is imported here and is binding:

@AGENTS.md

## Claude Code specifics

- **Slash commands.** Every folder in `.claude/skills/` is a `/<name>` command.
  - `/pr-opportunity-scan`, `/ollama-doctor`, and `/docs-sync` are manual only
    (`disable-model-invocation: true`). Claude never loads them on its own.
  - `/add-avatar-feature`, `/check-security-invariants`, and `/rust-optimize` can also
    trigger automatically from their descriptions.
  - Pair `/check-security-invariants` with the built-in `/security-review` on any
    networking, auth, or CI change.
- **Permissions.** `.claude/settings.json` pre-approves `./scripts/ci.sh`, the
  underlying `cargo fmt`/`clippy`/`test`/`check`/`build`/`fetch`/`tree`/`deny`
  commands, and read-only git. It denies reading `.env` files. Put personal
  additions in `.claude/settings.local.json` (gitignored).
- **Web sessions.** `.claude/hooks/session-start.sh` installs the pinned 1.88
  toolchain before the session starts, then warms the crate cache and test build in
  the background (log: `$CARGO_WARM_LOG`). If the first `cargo` command prints
  `Blocking waiting for file lock`, that is the warm-up finishing, not a hang.
- **Linux vs macOS.** Web sessions run Linux, so `app.rs` is not compiled. Say so
  whenever you report a green local gate for a change that touches AppKit code.
- **Editing these files.** Shared facts go in `AGENTS.md`; only Claude-specific
  wiring belongs here. `/docs-sync` keeps both in step with the source.
