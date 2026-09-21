# Build

Apple Silicon macOS 13+. Local builds use Rust 1.88 from `rust-toolchain.toml`;
CI uses the runner's current `stable`.

```sh
./scripts/ci.sh
./scripts/package-app.sh
open dist/CG-Agent-MacOS-Avatar.app
```

If `cargo clippy` binds an old rustup (1.85), use Homebrew's `cargo-clippy` instead. CI installs current `stable` via a SHA-pinned `dtolnay/rust-toolchain`.

The creature is a **client**. It does not start Ollama or CG-Agent-Harness. Run:

```sh
cgagentharness serve
```

so `/api/status` answers on loopback. Fresh homes use
`https://127.0.0.1:8790` with a home-specific certificate and account login;
legacy `tls.enabled: false` homes use HTTP. Avatar reads the certificate from
the selected Harness home. Default port is 8790, or the `port` field in
`~/.CGagentHarness/harness.json` if that file is a regular, non-world-writable
JSON file. See [Harness TLS and login](../README.md#harness-tls-and-login) for
the login and required password-replacement flow.

`CGAGENTHARNESS_HOME` is honored only when it is an absolute path with no `..` components. `.env` is never read.

The `.app` is ad-hoc signed (`codesign -s -`). Gatekeeper may require an Open anyway. Notarize later; this repo does not ship a Developer ID.
