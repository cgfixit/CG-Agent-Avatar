#!/bin/bash
# Local checks using rust-toolchain.toml; Cargo may fetch locked dependencies.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-targets
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny --locked check
else
  echo "cargo-deny not installed; dependency policy check skipped (required in CI)" >&2
fi
