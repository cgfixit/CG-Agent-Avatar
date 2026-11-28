#!/usr/bin/env bash
# Type-check and Clippy-lint the macOS-only code (src/app.rs, the NSWorkspace
# path in src/launch.rs) from a Linux host, without a macOS SDK.
#
# What this proves: the crate, its tests, and its dependencies type-check,
# borrow-check, and pass Clippy (-D warnings) for aarch64-apple-darwin.
# What it does NOT prove: that anything links, runs, or behaves correctly on
# macOS, or that the macOS unit tests pass. Nothing is linked here.
#
# The one obstacle is ring's build script, which compiles C for the target.
# It is satisfied with clang and a handful of stub headers created in a temp
# directory (never in the repo). The objects ring produces this way are NOT
# usable: never run `cargo build` or `cargo test` with this environment.
set -euo pipefail

target=aarch64-apple-darwin

if [[ "$(uname -s)" == "Darwin" ]]; then
  echo "check-macos: on macOS the normal gate already compiles app.rs;" \
    "run cargo clippy --locked --all-targets --all-features -- -D warnings." >&2
  exit 0
fi

if ! command -v clang >/dev/null 2>&1; then
  echo "check-macos: clang is required to cross-compile ring's C for $target" \
    "(e.g. apt-get install -y clang)." >&2
  exit 1
fi
if ! command -v ar >/dev/null 2>&1; then
  echo "check-macos: ar is required (e.g. apt-get install -y binutils)." >&2
  exit 1
fi

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo"

if ! rustup target list --installed | grep -qx "$target"; then
  rustup target add "$target"
fi

stub="$(mktemp -d "${TMPDIR:-/tmp}/cg-agent-macos-stub.XXXXXX")"
trap 'rm -rf "$stub"' EXIT
inc="$stub/usr/include"
mkdir -p "$inc"

# Stub headers: just enough for ring's C to compile with -ffreestanding.
# If a ring upgrade fails with "fatal error: '<name>.h' file not found",
# add that header here.
cat >"$inc/string.h" <<'EOF'
#pragma once
#include <stddef.h>
void *memcpy(void *dst, const void *src, size_t n);
void *memmove(void *dst, const void *src, size_t n);
void *memset(void *s, int c, size_t n);
int memcmp(const void *a, const void *b, size_t n);
size_t strlen(const char *s);
EOF
cat >"$inc/assert.h" <<'EOF'
#pragma once
#define assert(x) ((void)0)
EOF
cat >"$inc/stdlib.h" <<'EOF'
#pragma once
EOF
cat >"$inc/TargetConditionals.h" <<'EOF'
#pragma once
#define TARGET_OS_MAC 1
#define TARGET_OS_OSX 1
#define TARGET_OS_IPHONE 0
#define TARGET_OS_IOS 0
#define TARGET_OS_TV 0
#define TARGET_OS_WATCH 0
#define TARGET_OS_SIMULATOR 0
#define TARGET_OS_MACCATALYST 0
EOF

# A dedicated target dir keeps the unusable stub-built ring objects away from
# any real build output.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}/check-macos"
export CC_aarch64_apple_darwin=clang
export AR_aarch64_apple_darwin=ar
export SDKROOT="$stub"
export CFLAGS_aarch64_apple_darwin="--target=arm64-apple-macosx11.0 -ffreestanding -isystem $inc"

echo "check-macos: cargo clippy for $target (type-check + Clippy only; nothing is linked)"
if ! cargo clippy --locked --all-targets --all-features --target "$target" -- -D warnings; then
  echo >&2
  echo "check-macos: failed. If the error is in ring's build script (a missing" \
    "header or symbol), extend the stub headers in scripts/check-macos.sh;" \
    "otherwise it is a real error in the macOS-target code." >&2
  exit 1
fi
echo "check-macos: OK. Type-check and Clippy pass for $target. This does not prove" \
  "linking, running, AppKit behavior, or the macOS unit tests; macOS CI still must pass."
