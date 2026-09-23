#!/bin/bash
# Claude Code on the web: install the pinned toolchain and prefetch locked crates
# so fmt/Clippy/tests work offline-ish once the container snapshot is cached.
# Local sessions (macOS dev machines) are left alone.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

# rust-toolchain.toml pins the channel and components; `rustup show` installs them.
rustup show active-toolchain >/dev/null 2>&1 || rustup toolchain install
rustup component add rustfmt clippy >/dev/null

cargo fetch --locked
# Warm the test build so the first `cargo test` in a session is fast.
cargo test --locked --all-targets --no-run
