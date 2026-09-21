# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

`cg-agent` (CG-Agent-MacOS-Avatar) is a macOS menu-bar "creature" companion, written in Rust,
targeting Apple Silicon / macOS 13+. It is a **client only**: it talks over loopback to an
already-running [CG-Agent-Harness](https://github.com/cgfixit/CG-agent-harness) and/or a local
Ollama instance. It never starts either backend, never runs agent jobs, and never administers
harness accounts.

## Commands

```sh
./scripts/ci.sh              # local mirror of CI: fmt --check, clippy -D warnings, test, cargo deny
./scripts/package-app.sh     # produce dist/CG-Agent-MacOS-Avatar.app
open dist/CG-Agent-MacOS-Avatar.app
```

Equivalent individual commands:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings   # use Homebrew's cargo-clippy if rustup's is stuck on 1.85
cargo test --all-targets
cargo test --test source_contracts             # run a single integration test file
cargo test some_test_name                      # run a single test by name
cargo deny check                                # license/advisory checks, config in deny.toml
```

Local builds pin Rust 1.88 via `rust-toolchain.toml`; CI uses the runner's current `stable` via a
SHA-pinned `dtolnay/rust-toolchain`. The binary is macOS-only; on other platforms `main()` exits 2.

The built `.app` is ad-hoc codesigned (`codesign -s -`), not notarized, and is not committed to
git — recipients build it themselves.

## Architecture

The crate is `lib.rs` + `main.rs`, with `main` just calling `app::run()` on macOS. Modules under
`src/` are layered from "pure protocol/security logic" (portable, unit-tested on any OS) up to
"AppKit UI" (macOS-only):

- **`origin.rs`** — `LoopbackOrigin`: parses and validates a base URL as strictly loopback
  (`127.0.0.1` / `[::1]` only; `localhost` is rejected because it can resolve off-loopback).
  Also enforces the path allowlist from `paths.rs`.
- **`paths.rs`** — the exhaustive allowlist of HTTP paths the app is permitted to call
  (`/api/status`, `/api/sessions`, `/api/chat`, `/api/auth/login`, `/api/auth/password`, `/`), plus
  a documented `FORBIDDEN` list (agent/run, web/*, memory/*, soul, keys, etc.) that CI checks stay
  uncallable. Any new route must be added here deliberately.
- **`validate.rs`** — fail-closed checks on outbound data (message length/NUL bytes, session id
  charset/length) that run before any bytes leave the process.
- **`csrf.rs`** — extracts the CSRF token from the harness console HTML for guarded POSTs; never
  logs the token.
- **`http.rs`** — thin blocking HTTP helpers shared by `client.rs`/`discover.rs`, with a max body
  size (`MAX_BODY`).
- **`client.rs`** — the actual `Client`/`Status`/`ChatReply` used to call the harness: guarded
  POSTs carry the CSRF header, never send `"loop": true`, refuse redirects, and only hit allowlisted
  paths. Handles login (`/api/auth/login`) and self password-change (`/api/auth/password`) for a
  harness with `auth.enabled: true`, but cannot manage other accounts.
- **`home.rs`** — locates the harness home directory (default `~/.CGagentHarness`, overridable via
  `CGAGENTHARNESS_HOME` only when it's an absolute path with no `..` components) and reads its
  pinned TLS leaf certificate (`tls/server.pem`) and port (`harness.json`), with size limits. Never
  reads `.env`.
- **`discover.rs`** — fallback discovery when the headless port (8790 default) isn't listening,
  because the bundled `.app`'s sidecar binds an ephemeral port. Finds a user-owned
  `cgagentharness` listener via argv-only `lsof` and probes `GET /api/status`, without scanning the
  ephemeral port range or touching the desktop focus socket. Also defines the local Ollama port.
- **`ollama.rs`** — Direct Ollama backend: posts the bundled Soul system prompt to
  `POST http://127.0.0.1:11434/v1/chat/completions`. Stays local and tool-free (does not use
  Ollama's cloud web-search/fetch tools).
- **`launch.rs`** — the one deliberate exception to "does not start the harness": launches the
  bundled `CG Agent Harness.app` by its `CFBundleIdentifier` via macOS Launch Services when the
  user selects Harness mode. The bundle-id constant is portable/tested on any OS; the actual
  `NSWorkspace` call is `#[cfg(target_os = "macos")]`. Never a hardcoded path, never a spawned
  process. See `SECURITY.md`'s "Harness launch" row.
- **`mood.rs`** — pure derivation of a `Mood` enum (Asleep/Idle/Thinking/Talking/Sick) from harness
  status + in-flight chat state. No ML, just state mapping.
- **`theme.rs`** — the design system (layout/motion/color/type tokens) shared by every overlay
  surface, deliberately kept free of AppKit so it builds and its tests run on any platform; only
  `app.rs` converts its `Rgba` into `NSColor`. Two themes ship (`CLASSIC` default, `FABLE_PROTOCOL`),
  selectable via `CG_AGENT_THEME=classic|fable-protocol`.
- **`display.rs`** — display/window-sizing support used by the UI layer.
- **`app.rs`** (macOS-only, `#[cfg(target_os = "macos")]`) — the AppKit status-item/menu/overlay UI:
  wires the above modules into the actual menu bar creature, chat field, and reply bubble.

### Security posture baked into the architecture

This app is deliberately loopback-only and least-privilege by construction, not just by
convention — several modules exist specifically to make unsafe behavior structurally hard:
- `origin.rs` + `paths.rs` together make it impossible to address a non-loopback host or a
  non-allowlisted path through the normal `Client` API.
- TLS trust is pinned to a single certificate read from the harness's own home directory; the app
  never touches system/keychain trust and never disables certificate validation.
- Credentials/session cookies are not persisted to disk.
- `tests/source_contracts.rs`, `tests/ci_workflows.rs`, `tests/plist_contract.rs`, and
  `tests/lsof_argv.rs` are contract tests that lock down source-level invariants (e.g. forbidden
  paths staying unreferenced, CI workflow shape, Info.plist contents, argv-only `lsof` usage) —
  check these when touching networking, process-spawning, or CI config.

See `SECURITY.md` for the full threat model and `README.md` for user-facing behavior (menu items,
harness TLS/login flow, harness port discovery).
