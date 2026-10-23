#!/bin/bash
# Verify the packaged app without launching it or contacting a backend.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
app="${1:-$root/dist/CG-Agent-MacOS-Avatar.app}"
plist="$app/Contents/Info.plist"
plutil -lint "$plist"

assert_plist() {
  local actual
  actual=$(/usr/libexec/PlistBuddy -c "Print :$1" "$plist")
  [ "$actual" = "$2" ] || { echo "$1: expected $2, got $actual" >&2; exit 1; }
}
assert_plist CFBundleIdentifier com.cgfixit.cg-agent
assert_plist CFBundleExecutable cg-agent
assert_plist LSUIElement true
assert_plist NSAppTransportSecurity:NSAllowsLocalNetworking true
if /usr/libexec/PlistBuddy -c 'Print :NSAppTransportSecurity:NSAllowsArbitraryLoads' "$plist" >/dev/null 2>&1; then
  echo "NSAllowsArbitraryLoads must be absent" >&2
  exit 1
fi

binary="$app/Contents/MacOS/cg-agent"
[ -x "$binary" ] || { echo "missing executable: $binary" >&2; exit 1; }
[ -s "$app/Contents/Resources/creature.png" ] || { echo "missing creature artwork" >&2; exit 1; }
archs=$(lipo -archs "$binary")
[ "$archs" = arm64 ] || { echo "expected arm64, got: $archs" >&2; exit 1; }
codesign --verify --deep --strict --verbose=2 "$app"
echo "verified $app"
