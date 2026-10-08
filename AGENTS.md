# AGENTS.md

The single source of truth for every coding agent here: human maintainers,
**Claude Code** (CLI and web), and **OpenAI Codex** (CLI, IDE, cloud). Codex reads
it natively; Claude Code imports it from `CLAUDE.md`. Everything above the
tool-specific notes at the end applies to every agent.

## What this is

`cg-agent` (CG-Agent-MacOS-Avatar) is a macOS menu-bar "creature" companion written
in Rust. It targets Apple Silicon and macOS 13+. It is a **loopback-only client** of
an already-running [CG-Agent-Harness](https://github.com/cgfixit/CG-Agent-Harness)
and/or a local Ollama:
- It never starts Ollama or a bare Harness process; selecting Harness launches or
  activates the installed desktop app through Launch Services.
- It never runs agent jobs or administers harness accounts.

User-facing behavior is in `README.md`, and the threat model is in `SECURITY.md`.

## Commands

```sh
./scripts/ci.sh                                   # fmt + Clippy -D warnings + all tests (+ cargo-deny if installed)
cargo test --locked --test source_contracts       # one integration test file
cargo test --locked some_test_name                # one test by name
cargo deny --locked check                         # advisories/licenses/sources; config in deny.toml
./scripts/package-app.sh                          # macOS only: dist/CG-Agent-MacOS-Avatar.app (ad-hoc signed)
```

- Always pass `--locked`; CI rejects lockfile drift.
- `rust-toolchain.toml` pins Rust **1.88** (with rustfmt and Clippy). Use that
  toolchain's Clippy, not Homebrew's.
- CI tests stable Rust on Ubuntu and macOS, plus Rust 1.88 on macOS, and packages
  with 1.88. Every action is SHA-pinned. Audit tool versions and network needs are
  in `docs/BUILD.md`.
- The built `.app` is ad-hoc signed, not notarized, and never committed.

**Platform caveat.** The binary is macOS-only (`main()` exits 2 elsewhere). On
Linux, which includes Claude Code on the web and Codex cloud, `src/app.rs` and the
`NSWorkspace` path in `launch.rs` are compiled out: the Linux gate covers every
portable module and contract test but **cannot prove that an AppKit change
compiles**. Say so in any PR that touches `app.rs`, and never claim macOS CI passed
until it has.

## Architecture

The crate is `lib.rs` + `main.rs`; `main` just calls `app::run()` on macOS. Modules
are layered from pure, portable logic up to AppKit UI; put new code in the lowest
layer that can hold it.

**Layer 1: pure enforcement (portable, unit-tested anywhere)**
- **`origin.rs`**: `LoopbackOrigin` accepts only `127.0.0.1` / `[::1]`; `localhost`
  (which can resolve off-loopback), credentials, query, fragment, and extra path are
  rejected. It enforces a path allowlist on every URL it builds.
- **`paths.rs`**: the exhaustive Harness route allowlist:
  - `GET /`, `/api/status`, `/api/sessions`;
  - `POST /api/sessions`, `/api/chat`, `/api/auth/login`, `/api/auth/password`.

  It also holds a documented `FORBIDDEN` list (agent run/jobs/runs/schedules, MCP
  calls, config reload, model pull/select, chat cancel and attachments, session
  clear, `web/*`, `memory/*`, structured-memory purge/gates, soul and soul
  proposals, keys, logout, users, bootstrap password) that tests keep unreachable.
  Every entry is an exact path from the harness's `REGISTERED_PATHS`.
- **`validate.rs`**: fail-closed outbound checks before any bytes leave the process:
  message ≤ 32,768 chars after trim with no NUL, and a session-ID charset and length
  limit.
- **`csrf.rs`**: extracts the console CSRF token from `GET /` HTML, validates its
  charset and length, and redacts it in `Debug` so it never reaches logs.
- **`http.rs`**: bounded body reads (`MAX_BODY`, 1 MiB), shared by `client.rs`,
  `discover.rs`, and `ollama.rs`.

**Layer 2: backend I/O (blocking `reqwest`, no async runtime)**
- **`tls.rs`**: installs `ring` as the process-wide rustls crypto provider, once.
  `reqwest` runs with `rustls-no-provider` (the `rustls` feature would pull in
  `aws-lc-rs` and a C toolchain) and panics in `Client::build()` without one, so
  every builder in `client.rs`, `ollama.rs`, and `discover.rs` calls
  `tls::ensure_crypto_provider()` first.
- **`client.rs`**: the Harness `Client`. Guarded POSTs carry `X-CyClaw-CSRF`,
  never send `"loop"`, and refuse redirects; it handles login and self
  password-change for `auth.enabled: true` homes and pins a TLS home's leaf cert.
- **`home.rs`**: locates the harness home (default `~/.CGagentHarness`;
  `CGAGENTHARNESS_HOME` only as an absolute path with no `..`). It reads
  `harness.json` (port) and `tls/server.pem`, rejecting symlinks, files over 64 KiB,
  and world-writable files, and never reads `.env`.
- **`discover.rs`**: when the configured port (8790 by default) is silent, it finds
  the desktop sidecar's ephemeral port with argv-only
  `lsof -i4TCP@127.0.0.1 -sTCP:LISTEN` and a `GET /api/status` probe. It never scans
  a port range or touches the desktop focus socket, and skips Ollama's port
  (`OLLAMA_PORT`) and privileged ports. One pinned HTTPS client and one plain probe
  client are built per pass and re-pointed at each candidate (`Client::rebind`);
  every `reqwest` client owns a runtime thread, so never build one per port. A home
  with a pinned certificate never downgrades: `resolve_reachable` probes plain HTTP
  only when there is no `tls/server.pem`, whatever fallback the caller passes.
- **`ollama.rs`**: the Direct Ollama backend (the default).
  - It talks only to `127.0.0.1:11434`, with its own allowlist:
    `POST /v1/chat/completions`, `GET /api/tags`, and the daemon's
    `POST /api/experimental/web_search` / `web_fetch`.
  - The model tag is the constant `qwen3.8:27b-mlx`; `tags_ok` requires that
    exact tag, and a chat 404 with the tag absent becomes `ModelMissing`.
  - Each chat carries the bundled `resources/direct-ollama-system.md` as the
    system message plus the current message: no history, no tools. It sends
    `reasoning_effort: "none"` and Qwen's non-thinking sampling (temperature 0.7,
    top_p 0.8, presence_penalty 1.5), since Ollama's OpenAI route would otherwise
    think at "medium" and sample at 1.0. An explicit lookup phrase adds one search
    or page read, run through the local daemon and passed as untrusted text.
- **`web_intent.rs`** (pure): recognizes an explicit web lookup ("search the web
  for …", "look up …", "google …", "read <link>") and refuses private or
  credentialed links before any network call.
- **`launch.rs`**: the one deliberate exception to "does not start the harness":
  it opens `CG Agent Harness.app` by bundle ID `com.cgfixit.agent-harness` via
  Launch Services, never by path, spawned process, or arguments.

**Layer 3: portable UI state (no AppKit)**
- **`harness_setup.rs`**: typed Harness setup phases, advanced by a pinned loopback
  status response, login result, and required password change; generation checks
  discard results from earlier backend selections.
- **`mood.rs`**: pure `Mood` derivation (Asleep/Idle/Thinking/Talking/Sick) from
  backend status and in-flight chat state.
- **`theme.rs`**: design tokens (layout, motion, color, type). `CLASSIC` is the
  default, and `FABLE_PROTOCOL` is selected with `CG_AGENT_THEME=fable-protocol`
  (or `fable`).
- **`display.rs`**: treats model output as untrusted text. It strips C0/ANSI and
  bidi controls, and caps the bubble at 400 chars and the expanded view at 8,000.
  It never renders HTML and never opens URLs.

**Layer 4: AppKit (macOS only)**
- **`app.rs`** (`#[cfg(target_os = "macos")]`): status item, menu, overlay, chat
  field, and reply bubble. Network calls run on worker threads, never on the
  `NSApplication` run loop. Only this file converts `theme::Rgba` to `NSColor`.

## Hard rules (every agent)

1. **Loopback only.** Allow only `127.0.0.1` and `[::1]`, never `localhost` or a
   non-loopback host.
2. **Allowlist first.** A new HTTP route needs, in the same change, an entry in
   `src/paths.rs` (or `ollama.rs`'s `ALLOWED`), a row in `SECURITY.md`, and a
   contract test. Never call anything in `paths::FORBIDDEN`.
3. **Never weaken a control to get green.** This covers redirect refusal,
   single-cert TLS pinning (`tls_certs_only`, never `tls_certs_merge` or
   `danger_accept_invalid_certs`), the `ring` crypto provider installed by
   `tls.rs`, the `MAX_BODY` cap, CSRF on guarded POSTs, argv-only `lsof`, and
   asymmetric timeouts. Fix the regression, never the test.
4. **No secrets.** Never read `.env` or `CGAGENTHARNESS_API_KEY`. Never persist
   credentials or cookies, and never log the CSRF token or request bodies.
5. **No process spawning for the harness.** Launch it only by bundle ID, never with
   `Command::new`, and never with `sh -c` anywhere.
6. **Respect the layers** above: AppKit only in `app.rs`, blocking I/O never on the
   main thread.
7. **Workflows are contract-tested.** Keep actions SHA-pinned and `permissions`
   minimal. Never use `pull_request_target`.
8. **Docs follow code.** When behavior changes, rewrite the owning doc (doc map
   below) in place, not as an appended note; `docs-sync` does this.

## Contract tests

`include_str!` source-grep and behavior tests are the enforcement mechanism, not
style checks. Read the relevant one before touching networking, process spawning,
`Info.plist`, or CI.

| File | Locks down |
|---|---|
| `tests/source_contracts.rs` | no agent routes or `loop` field in `client.rs`; no forwarding headers; no dotenv in `home.rs`; `FORBIDDEN` unreachable; discovery never scans or touches the focus socket; launch by bundle ID only; Ollama loopback, OpenAI-compatible only; web lookups only via the loopback daemon (no cloud host, key, or tools) |
| `tests/objections.rs` | SSRF baits rejected, CSRF injection shapes, session IDs shaped like paths, oversized bodies, token redaction in `Debug` |
| `tests/lsof_argv.rs` | argv-only loopback `lsof`; Ollama and privileged ports skipped |
| `tests/plist_contract.rs` | bundle ID `com.cgfixit.cg-agent`, `LSUIElement` (no Dock), ATS local networking only |
| `tests/tls_pin.rs` | the Harness TLS pin with real loopback handshakes (certificates minted at run time with the `rcgen` dev-dependency, `ring` only): the pinned certificate is accepted; a different certificate, or one that doesn't cover loopback, is `CertMismatch`; a plaintext listener is never read, and `discover::resolve_reachable` never returns `Http` for a home with a pinned certificate |
| `tests/ci_workflows.rs` | every workflow under `.github/workflows` (discovered, not listed): every `uses` SHA-pinned; permissions `contents: read` except `contents: write` on `bundle.yml`'s `release` job; no `pull_request_target`; checkouts never persist credentials; every job has a timeout; caches written only from `main`; the stable leg really runs stable; pinned cargo-audit; lockfile drift rejected; release toolchain tested on macOS; bundle schedule and dispatch, with a manual `main` run publishing a full Latest release |

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

- Commits use a short type prefix (`perf:`, `ci:`, `docs:`); older history is
  free-form.
- One concern per PR, draft until CI is green. Say when a change was verified only
  on Linux.
- Never commit `dist/`, `target/`, or a built `.app`.

## Skills

Skills are Markdown playbooks in `.claude/skills/<name>/SKILL.md` (open Agent
Skills format); `.agents/skills` symlinks to the same directory, so Claude Code
(`/<name>`) and Codex (`$<name>`) share one copy.

| Skill | Invocation | Use it for |
|---|---|---|
| `add-avatar-feature` | auto or manual | a new menu item, mood/theme state, or backend call in the right layer, with allowlist + `SECURITY.md` + tests moving together |
| `check-security-invariants` | auto or manual | a diff checked against `SECURITY.md` and the contract tests before pushing networking/auth/discovery/plist/CI changes (pair with a generic security review) |
| `rust-optimize` | auto or manual | a Clippy-driven perf/idiom pass that respects the blocking-I/O-off-main-thread and `MAX_BODY` design |
| `pr-opportunity-scan` | **manual only** | a ~3-minute read-only scan proposing 3-4 focused PRs |
| `ollama-doctor` | **manual only** | Direct Ollama config, model-tag readiness, system-prompt fitness, and bubble symptoms |
| `docs-sync` | **manual only** | rewriting stale docs in place to match the source (`--check` for report-only) |

Manual-only skills set `disable-model-invocation: true` for Claude Code and carry
`agents/openai.yaml` with `allow_implicit_invocation: false` for Codex. A new skill
needs `name` and `description` frontmatter, real test names, and a row above.

## Claude Code notes

`CLAUDE.md` imports this file and owns the Claude-only wiring: the `.claude/settings.json`
permissions, the `.env` read denial, and the web-session SessionStart hook.

## Codex notes

- Codex reads this file natively; no nested `AGENTS.md` exists to override it.
- Skills are discovered through `.agents/skills`; auto-invocable ones may trigger
  from their `description`.
- Codex cloud and sandboxed runs are Linux (platform caveat above). Run
  `./scripts/ci.sh` before proposing a change; a missing `cargo-deny` prints an
  explicit skip and CI still enforces it.
- Nothing in `.claude/settings.json` or the hook applies to Codex; follow the hard
  rules directly.
