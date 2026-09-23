# AGENTS.md

Shared ground rules for every coding agent that contributes to this repository.
Active contributors: human maintainers, **Claude Code** (local CLI and Claude Code on
the web), and any other agent that reads `AGENTS.md`. `CLAUDE.md` holds the detailed
architecture notes; this file is the shared contract all agents follow.

## Project in one paragraph

`cg-agent` is a macOS menu-bar creature written in Rust (Apple Silicon, macOS 13+).
It is a **loopback-only client** of an already-running CG-Agent-Harness and/or local
Ollama. It never starts Ollama or a bare Harness process, never runs agent jobs, and
never administers harness accounts. Its security posture is set by the code itself,
and contract tests enforce it. See `SECURITY.md` for the threat model.

## Commands

```sh
./scripts/ci.sh                                         # fmt + Clippy -D warnings + tests (+ cargo-deny if installed)
cargo test --locked --test source_contracts             # a single integration test file
cargo test --locked some_test_name                      # a single test
./scripts/package-app.sh                                # macOS only: dist/CG-Agent-MacOS-Avatar.app
```

Always pass `--locked`, because CI rejects lockfile drift. `rust-toolchain.toml` pins
Rust 1.88.

**On Linux** (including Claude Code on the web), `src/app.rs` and the macOS paths in
`launch.rs` are compiled out. The gate still covers the portable modules and all
contract tests, but it cannot prove an AppKit change compiles. Say so in the PR when
the change touches `app.rs`. Do not claim that macOS CI passed until it has.

## Hard rules (every agent)

1. **Loopback only.** Only `127.0.0.1` and `[::1]` are allowed. `localhost` is rejected.
   Never add a non-loopback host.
2. **Path allowlist.** A new HTTP route requires edits to `src/paths.rs`, a new row in
   `SECURITY.md`, and a contract test in `tests/source_contracts.rs`, all in the same
   change. Never call anything in `paths::FORBIDDEN`.
3. **Never weaken a control to get green.** Redirect refusal, certificate pinning
   (`tls/server.pem` from the harness home), the `MAX_BODY` response cap, CSRF on
   guarded POSTs, and argv-only `lsof` all stay. Fix the regression, never the test.
4. **Never read secrets.** Do not read `.env` or `CGAGENTHARNESS_API_KEY`. Never
   persist credentials or session cookies to disk. Never log the CSRF token.
5. **No process spawning for the harness.** Launch it only via Launch Services by
   bundle ID (`launch.rs`). Never use `Command::new` and never hardcode a path.
6. **Stay in your layer.** Pure logic (`origin`, `paths`, `validate`, `csrf`, `http`)
   goes at the bottom, then I/O (`client`, `discover`, `home`, `ollama`), then portable
   UI state (`mood`, `theme`, `display`). AppKit wiring goes only in `app.rs`.
7. **CI and workflow files are contract-tested** (`tests/ci_workflows.rs`). Actions stay
   SHA-pinned, and no workflow uses `pull_request_target`.

## Contract tests to check before pushing

| File | Locks down |
|---|---|
| `tests/source_contracts.rs` | forbidden routes, the chat JSON shape, no forwarding headers, no dotenv, discovery/launch/Ollama shape |
| `tests/objections.rs` | SSRF baits, CSRF junk, session-ID paths, oversized bodies, token redaction |
| `tests/lsof_argv.rs` | argv-only loopback `lsof`, skipped ports |
| `tests/plist_contract.rs` | bundle ID, no Dock icon, ATS local-networking only |
| `tests/ci_workflows.rs` | audit/bundle workflow shape, lockfile drift, release toolchain |

## Git and PR conventions

- Use conventional-ish commit subjects as seen in history (`perf:`, `ci:`, `docs:`, `fix:`, `feat:`).
- Keep changes small, one concern per PR, and open PRs as drafts until CI is green.
- Never commit `dist/`, `target/`, or a built `.app`.

## Claude Code

Claude Code reads `CLAUDE.md` automatically. That file imports this one, so these
rules apply to Claude as well. Project wiring lives under `.claude/`:

```
.claude/
├── settings.json                  # shared permissions + SessionStart hook registration
├── hooks/
│   └── session-start.sh           # web only: install pinned toolchain, cargo fetch --locked, prebuild tests
└── skills/
    ├── add-avatar-feature/        # put new features in the right layer, move allowlist/SECURITY.md/tests together
    ├── check-security-invariants/ # compare a diff against SECURITY.md + contract tests before pushing
    └── rust-optimize/             # Clippy-driven perf/idiom pass that respects the blocking-I/O + MAX_BODY design
```

- **Skills** trigger automatically from their `description` frontmatter, or can be
  invoked by name (`/check-security-invariants`, etc.). Pick the skill that matches
  the task:
  - Adding or changing behavior: `add-avatar-feature`
  - Before pushing anything that touches networking, auth, discovery, `Info.plist`,
    or workflows: `check-security-invariants`, **plus** the built-in `/security-review`
  - Performance or cleanup requests: `rust-optimize`
- **settings.json** pre-approves the read-only git commands and the cargo commands
  that `ci.sh` runs. It denies reading `.env` files. Personal overrides go in
  `.claude/settings.local.json`, which is gitignored.
- **SessionStart hook** runs only when `CLAUDE_CODE_REMOTE=true` and does nothing on
  local machines. It runs synchronously, so the toolchain and crates are ready before
  the first command.
- To add a new skill, create `.claude/skills/<kebab-name>/SKILL.md` with `name` and
  `description` frontmatter. Base it on current source and cite real test names,
  and list it in the tree above.
