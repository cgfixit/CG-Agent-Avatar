# Build and verify

The native app targets Apple Silicon macOS 13+. Install Apple's Command Line
Tools (`xcode-select --install`) and Rust through rustup. Run commands from the
repository root with rustup's binaries first on `PATH`:

```sh
export PATH="$HOME/.cargo/bin:$PATH"
rustup show active-toolchain
./scripts/ci.sh
./scripts/package-app.sh
open dist/CG-Agent-MacOS-Avatar.app
```

`rust-toolchain.toml` selects Rust **1.88**, including rustfmt and Clippy. Use
`cargo clippy` from that toolchain; mixing a Homebrew Clippy with rustup's Cargo
can produce incompatible compiler metadata. Packaging uses the same pin and
`--locked`, so a missing or outdated `Cargo.lock` fails instead of changing the
dependency graph during a build.

The package script creates `dist/CG-Agent-MacOS-Avatar.app`, replaces an existing
bundle at that output path, and ad-hoc signs it. It is not notarized, and neither
the app nor build output belongs in git. Validate the result with:

```sh
codesign --verify --deep --strict dist/CG-Agent-MacOS-Avatar.app
plutil -lint dist/CG-Agent-MacOS-Avatar.app/Contents/Info.plist
```

## Local checks and hosted CI

`./scripts/ci.sh` runs formatting, Clippy with warnings denied, and all test
targets. On Linux, including Claude Code on the web and Codex cloud, the same
script covers the portable modules and every contract test, but `src/app.rs` is
compiled only on macOS. AppKit changes therefore need the macOS CI jobs. If `cargo-deny` is installed, it also checks dependency advisories,
licenses, sources, and version policy; otherwise it prints an explicit skip.
Install the audit tools separately from the app's Rust toolchain:

```sh
cargo +stable install cargo-deny --locked --version 0.20.2
cargo +stable install cargo-audit --locked --version 0.22.2
cargo deny --locked check
cargo audit
```

Cargo Deny 0.20.2 matches the pinned action's bundled version. Older local
versions can fail to parse current advisories (for example CVSS 4.0). Update the
tool when that happens; do not remove advisories or weaken `deny.toml`.

Dependency fetching and advisory refreshes need network access. The HTTP tests
also bind loopback sockets. With dependencies already cached,
`CARGO_NET_OFFLINE=true ./scripts/ci.sh` suppresses Cargo registry access, but
Cargo Deny's advisory fetch is a separate operation; `cargo deny --offline
--locked check` uses cached policy data and cannot establish advisory freshness.

| Check | Local | GitHub Actions |
|---|---|---|
| fmt, Clippy, tests | Rust 1.88, current host | Current stable on Ubuntu and macOS |
| Release-toolchain compatibility | Same pinned local toolchain | Rust 1.88 tests on macOS, including AppKit |
| Release build | `./scripts/package-app.sh` | Stable in `ci`; Rust 1.88 in `bundle` |
| Dependency policy | Cargo Deny when installed | Required `cargo-deny` job |
| Vulnerability audit | `cargo audit` | Pinned Cargo Audit 0.22.2 |
| Secret scan | Run Gitleaks separately | Pinned Gitleaks with archive checksum verification |
| Native interaction/screenshots | Manual Computer Use on the built app | Not covered by hosted unit tests |

All dependency-resolving build/test commands use `--locked`. Review intentional
updates to `Cargo.toml` and `Cargo.lock` together, then rerun checks; a passing
locked build verifies the committed resolution, not that every crate is the
newest published version. Dependabot checks Cargo and Actions weekly.

## Backend setup

Direct Ollama is selected on launch. Avatar expects a local service on
`http://127.0.0.1:11434` exposing `/api/tags` and `/v1/chat/completions`, with the
fixed model tag `qwen3.8:27b-mlx`. Start that service separately; Avatar does not
install models or start Ollama. Optional
[web lookups](../README.md#web-lookups-direct-ollama) also need Ollama 0.18.1 or
newer, signed in with `ollama signin`.

Selecting Harness in the menu launches or activates the installed
`CG Agent Harness.app` through Launch Services. For a headless setup, start:

```sh
cgagentharness serve
```

Use one Harness instance per home. The desktop app uses an ephemeral port;
headless serving normally uses 8790 or the `port` in `harness.json`. Avatar probes
the configured port and discovers desktop listeners with `lsof`.

`CGAGENTHARNESS_HOME` selects the home only when it is an absolute path without
`..` components; the default is `~/.CGagentHarness`. Avatar reads the port and
pinned TLS certificate from that home, never `.env`. For a temporary test home:

```sh
CGAGENTHARNESS_HOME=/absolute/path/to/test-home \
  ./dist/CG-Agent-MacOS-Avatar.app/Contents/MacOS/cg-agent
```

TLS-enabled homes require the matching `tls/server.pem`. Account-gated homes
also require login in Avatar; see [Harness setup](../README.md#harness-tls-and-login).
Only test a password replacement against an account you intend to change.
