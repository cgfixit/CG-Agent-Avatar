#!/bin/bash
# Local mirror of .github/workflows/ci.yml (no network).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
if command -v cargo-clippy >/dev/null 2>&1; then
  cargo-clippy --all-targets --all-features -- -D warnings
else
  cargo clippy --all-targets --all-features -- -D warnings
fi
cargo test --all-targets
if command -v cargo-deny >/dev/null 2>&1; then
  cargo deny check
fi
