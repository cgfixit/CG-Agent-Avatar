#!/bin/bash
# Build an ad-hoc-signed Apple Silicon CG-Agent-MacOS-Avatar.app. Not notarized.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
app="CG-Agent-MacOS-Avatar.app"
cargo build --release
rm -rf "$root/dist/CG-Agent.app" "$root/dist/$app"
dist="$root/dist/$app/Contents"
mkdir -p "$dist/MacOS" "$dist/Resources"
cp "$root/target/release/cg-agent" "$dist/MacOS/cg-agent"
cp "$root/resources/Info.plist" "$dist/Info.plist"
cp "$root/assets/creature.png" "$dist/Resources/creature.png"
if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - "$root/dist/$app"
fi
echo "built $root/dist/$app"
