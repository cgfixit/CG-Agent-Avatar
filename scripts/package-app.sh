#!/bin/bash
# Build an ad-hoc-signed Apple Silicon CG-Agent-MacOS-Avatar.app. Not notarized.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
app="CG-Agent-MacOS-Avatar.app"
# Cargo reports the real executable path, including target-dir and target-triple overrides.
binary="$(cargo build --locked --release --bin cg-agent --message-format=json-render-diagnostics | python3 -c '
import json, sys
paths = [message["executable"] for line in sys.stdin
         if (message := json.loads(line)).get("reason") == "compiler-artifact"
         and message.get("target", {}).get("name") == "cg-agent"
         and "bin" in message["target"].get("kind", [])
         and message.get("executable")]
if len(paths) != 1:
    sys.exit("expected one cg-agent executable from Cargo")
print(paths[0])
')"
[ -f "$binary" ] || { echo "Cargo executable is missing: $binary" >&2; exit 1; }
rm -rf "$root/dist/CG-Agent.app" "$root/dist/$app"
dist="$root/dist/$app/Contents"
mkdir -p "$dist/MacOS" "$dist/Resources"
cp "$binary" "$dist/MacOS/cg-agent"
cp "$root/resources/Info.plist" "$dist/Info.plist"
cp "$root/assets/creature.png" "$dist/Resources/creature.png"
if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - "$root/dist/$app"
fi
echo "built $root/dist/$app"
