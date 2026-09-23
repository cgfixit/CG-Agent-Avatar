#!/bin/bash
# Claude Code on the web: make the pinned toolchain available before the session
# starts, then warm the crate cache and test build in the background.
# Local sessions (macOS dev machines) are left alone.
#
# Split on purpose:
#   - rustup install stays synchronous: concurrent rustup installs are not
#     reliably safe, and the first `cargo` call would otherwise race it.
#   - cargo fetch + test prebuild run detached: Cargo serializes on its own
#     package/build locks, so an early `cargo test` just waits instead of failing.
# Background progress is logged to $CARGO_WARM_LOG.
set -euo pipefail

if [ "${CLAUDE_CODE_REMOTE:-}" != "true" ]; then
  exit 0
fi

cd "${CLAUDE_PROJECT_DIR:-$(dirname "$0")/../..}"

# rust-toolchain.toml pins the channel and components.
rustup show active-toolchain >/dev/null 2>&1 || rustup toolchain install
rustup component add rustfmt clippy >/dev/null

CARGO_WARM_LOG="${TMPDIR:-/tmp}/cg-agent-cargo-warm.log"
echo "export CARGO_WARM_LOG=\"$CARGO_WARM_LOG\"" >> "${CLAUDE_ENV_FILE:-/dev/null}"

setsid nohup bash -c '
  cargo fetch --locked &&
  cargo test --locked --all-targets --no-run &&
  echo "cargo warm: done"
' >"$CARGO_WARM_LOG" 2>&1 < /dev/null &
