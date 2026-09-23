# AGENTS.md

This is the single source of truth for every coding agent working in this repository.
Active contributors are human maintainers, **Claude Code** (local CLI and Claude Code
on the web), and **OpenAI Codex** (CLI, IDE, and cloud). Codex reads this file
natively. Claude Code reads it through the `@AGENTS.md` import in `CLAUDE.md`.
Tool-specific notes are at the end. Everything above them applies to every agent.

## What this is

`cg-agent` (CG-Agent-MacOS-Avatar) is a macOS menu-bar "creature" companion written
in Rust. It targets Apple Silicon and macOS 13+. It is a **loopback-only client** of
an already-running [CG-Agent-Harness](https://github.com/cgfixit/CG-Agent-Harness)
and/or a local Ollama:
- It never starts Ollama or a bare Harness process. Selecting Harness launches or
  activates the installed desktop app through Launch Services.
- It never runs agent jobs and never administers harness accounts.

User-facing behavior is in `README.md`, and the threat model is in `SECURITY.md`.

## Commands

```sh
./scripts/ci.sh                                   # fmt + Clippy -D warnings + all tests (+ cargo-deny if installed)
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
cargo test --locked --test source_contracts       # one integration test file
cargo test --locked some_test_name                # one test by name
cargo deny --locked check                         # advisories/licenses/sources; config in deny.toml
./scripts/package-app.sh                          # macOS only: dist/CG-Agent-MacOS-Avatar.app (ad-hoc signed)
```

- Always pass `--locked`, because CI rejects lockfile drift.
- `rust-toolchain.toml` pins Rust **1.88** (with rustfmt and Clippy). Put rustup's
  binaries first on `PATH` and use that toolchain's Clippy, not Homebrew's.
- CI tests stable Rust on Ubuntu and macOS, plus Rust 1.88 on macOS. Packaging uses
  1.88. Every action is pinned to a full SHA.
- Audit tool versions and network needs are in `docs/BUILD.md`.
- The built `.app` is ad-hoc signed (`codesign -s -`) and not notarized. It is never
  committed; recipients build it themselves.

**Platform caveat.** The binary is macOS-only: on other platforms `main()` exits 2.
On Linux, which includes Claude Code on the web and Codex cloud, `src/app.rs` and the
`NSWorkspace` path in `launch.rs` are compiled out. The Linux gate still covers every
portable module and all contract tests, but it **cannot prove that an AppKit change
compiles**. Say so in the PR whenever you touch `app.rs`, and never claim that macOS
CI passed until it has.

## Architecture

The crate is `lib.rs` + `main.rs`; `main` just calls `app::run()` on macOS. Modules
are layered from pure, portable logic up to AppKit UI. Put new code in the lowest
layer that can hold it.

**Layer 1: pure enforcement (portable, unit-tested anywhere)**
- **`origin.rs`**: `LoopbackOrigin` accepts only `127.0.0.1` / `[::1]`. `localhost`
  is rejected because it can resolve off-loopback. So are credentials, query,
  fragment, and extra path. It also enforces a path allowlist on every URL it builds.
- **`paths.rs`**: the exhaustive Harness route allowlist:
  - `GET /`, `/api/status`, `/api/sessions`;
  - `POST /api/sessions`, `/api/chat`, `/api/auth/login`, `/api/auth/password`.

  It also holds a documented `FORBIDDEN` list (agent run/jobs, `web/*`, `memory/*`,
  soul, keys, logout, users) that tests keep unreachable.
- **`validate.rs`**: fail-closed outbound checks, run before any bytes leave the
  process: message ≤ 32,768 chars after trim with no NUL, and a session-ID charset
  and length limit.
- **`csrf.rs`**: extracts the console CSRF token from `GET /` HTML. It validates the
  charset and length, and `Debug` redacts the token, so it never appears in logs.
- **`http.rs`**: bounded body reads (`MAX_BODY`, 1 MiB), shared by `client.rs`,
  `discover.rs`, and `ollama.rs`.

**Layer 2: backend I/O (blocking `reqwest`, no async runtime)**
- **`client.rs`**: the Harness `Client`:
  - guarded POSTs carry `X-CyClaw-CSRF`, never send `"loop"`, and refuse redirects;
  - it handles login and self password-change for `auth.enabled: true` homes;
  - for TLS homes it pins the home's leaf cert.
- **`home.rs`**: locates the harness home. The default is `~/.CGagentHarness`.
  `CGAGENTHARNESS_HOME` is honored only as an absolute path with no `..`. It reads
  `harness.json` (port) and `tls/server.pem`, rejecting symlinks, files over 64 KiB,
  and world-writable files. It never reads `.env`.
- **`discover.rs`**: when the configured port (8790 by default) is silent, it finds
  the desktop sidecar's ephemeral port by running argv-only
  `lsof -i4TCP@127.0.0.1 -sTCP:LISTEN` and probing `GET /api/status`. It never scans a
  port range, never touches the desktop focus socket, and skips Ollama's port
  (`OLLAMA_PORT`) and privileged ports.
- **`ollama.rs`**: the Direct Ollama backend (the default).
  - It talks only to `127.0.0.1:11434`, with its own allowlist:
    `POST /v1/chat/completions`, `GET /api/tags`, and the daemon's
    `POST /api/experimental/web_search` / `web_fetch`.
  - The model tag is the constant `qwen3.8:27b-mlx`; `tags_ok` requires that
    exact tag, and a chat 404 with the tag absent becomes `ModelMissing`.
  - Each chat carries the bundled `resources/direct-ollama-system.md` as the
    system message plus the current message: no history, no tools. When the
    message starts with an explicit lookup phrase, it also carries one search or
    page read, run through the local daemon and passed as untrusted text.
- **`web_intent.rs`** (pure): decides whether a message is an explicit web
  lookup ("search the web for …", "look up …", "google …", "read <link>") and
  refuses private or credentialed links before any network call.
- **`launch.rs`**: the one deliberate exception to "does not start the harness". It
  launches `CG Agent Harness.app` by bundle ID `com.cgfixit.agent-harness` via
  Launch Services. There is never a hardcoded path, a spawned process, or arguments.

**Layer 3: portable UI state (no AppKit)**
- **`mood.rs`**: pure `Mood` derivation (Asleep/Idle/Thinking/Talking/Sick) from
  backend status and in-flight chat state.
- **`theme.rs`**: design tokens (layout, motion, color, type). `CLASSIC` is the
  default, and `FABLE_PROTOCOL` is selected with `CG_AGENT_THEME=fable-protocol`
  (or `fable`).
- **`display.rs`**: treats model output as untrusted text. It strips C0/ANSI and caps
  the bubble at 400 chars and the expanded view at 8,000. It never renders HTML and
  never opens URLs.

**Layer 4: AppKit (macOS only)**
- **`app.rs`** (`#[cfg(target_os = "macos")]`): the status item, menu, overlay, chat
  field, and reply bubble. Network calls run on worker threads, never on the
  `NSApplication` run loop. Only this file converts `theme::Rgba` to `NSColor`.

## Hard rules (every agent)

1. **Loopback only.** Allow only `127.0.0.1` and `[::1]`, never `localhost`, and
   never a non-loopback host.
2. **Allowlist first.** A new HTTP route needs three things in the same change: an
   entry in `src/paths.rs` (or `ollama.rs`'s `ALLOWED`), a row in `SECURITY.md`, and
   a contract test. Never call anything in `paths::FORBIDDEN`.
3. **Never weaken a control to get green.** This covers redirect refusal,
   single-cert TLS pinning (`tls_built_in_root_certs(false)`, never
   `danger_accept_invalid_certs`), the `MAX_BODY` cap, CSRF on guarded POSTs,
   argv-only `lsof`, and asymmetric timeouts. Fix the regression, never the test.
4. **No secrets.** Never read `.env` or `CGAGENTHARNESS_API_KEY`. Never persist
   credentials or cookies, and never log the CSRF token or request bodies.
5. **No process spawning for the harness.** Launch it only by bundle ID, never with
   `Command::new`, and never with `sh -c` anywhere.
6. **Respect the layers** above. AppKit goes only in `app.rs`. Blocking I/O never runs
   on the main thread.
7. **Workflows are contract-tested.** Keep actions SHA-pinned and `permissions`
   minimal. Never use `pull_request_target`.
8. **Docs follow code.** When behavior changes, rewrite the owning doc in place,
   not as an appended note. See the doc map below or run `docs-sync`.

## Contract tests

These are `include_str!` source-grep and behavior tests that act as the enforcement
mechanism, not style checks. Read the relevant one before touching networking,
process spawning, `Info.plist`, or CI.

| File | Locks down |
|---|---|
| `tests/source_contracts.rs` | no agent routes or `loop` field in `client.rs`; no forwarding headers; no dotenv in `home.rs`; `FORBIDDEN` unreachable; discovery never scans or touches the focus socket; launch by bundle ID only; Ollama loopback, OpenAI-compatible only; web lookups only via the loopback daemon (no cloud host, key, or tools) |
| `tests/objections.rs` | SSRF baits rejected, CSRF injection shapes, session IDs shaped like paths, oversized bodies, token redaction in `Debug` |
| `tests/lsof_argv.rs` | argv-only loopback `lsof`; Ollama and privileged ports skipped |
| `tests/plist_contract.rs` | bundle ID `com.cgfixit.cg-agent`, `LSUIElement` (no Dock), ATS local networking only |
| `tests/ci_workflows.rs` | pinned cargo-audit; no `pull_request_target`; bundle schedule and dispatch; lockfile drift rejected; release toolchain tested on macOS |

## Doc map (one owner per fact)

| Doc | Owns |
|---|---|
| `README.md` | what the app does, backends, setup, TLS/login flow, network boundaries |
| `docs/CONTROLS.md` | menu items, bubble messages, themes, troubleshooting |
| `docs/BUILD.md` | toolchain, local vs hosted checks, audit tool versions, packaging |
| `SECURITY.md` | asset → threat → control table, discovery/TLS/Ollama rules, non-goals |
| `AGENTS.md` | this file: agent rules, architecture, contract-test map, skills |
| `CLAUDE.md` | Claude Code-only wiring (imports this file) |

## Git and PR conventions

- Recent commits use a short type prefix (`perf:`, `ci:`, `docs:`). Use one for new
  commits; older history is free-form.
- One concern per PR, opened as a draft until CI is green. Mention it when a change
  could only be verified on Linux.
- Never commit `dist/`, `target/`, or a built `.app`.

## Skills

Skills are Markdown playbooks in `.claude/skills/<name>/SKILL.md`, using the open
Agent Skills format. `.agents/skills` is a symlink to the same directory, so Claude
Code and Codex share one copy.

| Skill | Invocation | Use it for |
|---|---|---|
| `add-avatar-feature` | auto or manual | a new menu item, mood/theme state, or backend call, put in the right layer with allowlist + `SECURITY.md` + tests moving together |
| `check-security-invariants` | auto or manual | a diff checked against `SECURITY.md` and the contract tests before pushing networking/auth/discovery/plist/CI changes (pair it with a generic security review) |
| `rust-optimize` | auto or manual | a Clippy-driven perf/idiom pass that respects the blocking-I/O-off-main-thread and `MAX_BODY` design |
| `pr-opportunity-scan` | **manual only** | a ~3-minute read-only scan that proposes 3-4 focused PRs (bugs, deps, CI, tests, Ollama, perf, security) |
| `ollama-doctor` | **manual only** | diagnosing Direct Ollama config, the model-tag readiness check, system-prompt fitness, and bubble symptoms |
| `docs-sync` | **manual only** | rewriting stale docs in place to match the current source (`--check` for report-only) |

How to invoke: `/<name>` in Claude Code, `$<name>` in Codex. The manual-only skills
set `disable-model-invocation: true` for Claude Code, and each has
`agents/openai.yaml` with `allow_implicit_invocation: false` for Codex.

To add a skill, create `.claude/skills/<kebab-name>/SKILL.md` with `name` and
`description` frontmatter. Base it on current source and cite real test names. Add
`agents/openai.yaml` if it should be manual-only, and add a row to the table above.

## Claude Code notes

- `CLAUDE.md` imports this file; keep Claude-only details there.
- `.claude/settings.json` does three things:
  - pre-approves the cargo commands that `ci.sh` runs, plus read-only git;
  - denies reading `.env` files;
  - registers the SessionStart hook.

  Personal overrides go in `.claude/settings.local.json`, which is gitignored.
- `.claude/hooks/session-start.sh` runs only on the web (`CLAUDE_CODE_REMOTE=true`).
  It installs the pinned toolchain synchronously, then warms `cargo fetch` and the
  test build in the background, logging to `$CARGO_WARM_LOG`. An early `cargo`
  command waits on Cargo's lock instead of failing.

## Codex notes

- Codex reads this file natively. A nested `AGENTS.md` would override it for its
  subtree; none exist today.
- Skills are discovered through `.agents/skills` (the symlink). Invoke them with
  `$<name>`. The auto-invocable ones may trigger implicitly from their
  `description`.
- Codex cloud and sandboxed runs are Linux, so the platform caveat above applies.
  Run `./scripts/ci.sh` before proposing a change. `cargo-deny` may be missing
  there; the script prints an explicit skip, and CI still enforces it.
- Nothing in `.claude/settings.json` or the hook applies to Codex. Follow the hard
  rules above directly.
