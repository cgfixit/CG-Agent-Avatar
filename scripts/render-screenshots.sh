#!/bin/bash
# Re-render screenshots/*.png from screenshots/scenes.html with headless
# Chromium (2x). The scenes are synthetic illustrations built from the theme
# tokens in src/theme.rs, not native app captures. Linux or macOS.
# (687 = 600pt viewport + headless window chrome; needs Python Pillow.) Override the browser with CHROME=/path/to/chrome.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
chrome="${CHROME:-}"
if [ -z "$chrome" ]; then
  for c in /opt/pw-browsers/chromium/chrome-linux/chrome \
           /opt/pw-browsers/chromium-*/chrome-linux/chrome \
           "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
           chromium google-chrome; do
    if command -v "$c" >/dev/null 2>&1 || [ -x "$c" ]; then chrome="$c"; break; fi
  done
fi
[ -n "$chrome" ] || { echo "no Chrome/Chromium found; set CHROME=" >&2; exit 1; }
for scene in hero thinking expanded fable menu notice; do
  "$chrome" --headless=new --no-sandbox --disable-gpu --hide-scrollbars \
    --force-device-scale-factor=2 --window-size=960,687 --virtual-time-budget=2000 \
    --screenshot="$root/screenshots/$scene.png" \
    "file://$root/screenshots/scenes.html#$scene" >/dev/null 2>&1
  # The headless window is taller than the 600pt page; keep the page only.
  python3 -c 'import sys; from PIL import Image; f=sys.argv[1]; i=Image.open(f); i.crop((0,0,1920,1200)).save(f, optimize=True)' "$root/screenshots/$scene.png"
  echo "rendered screenshots/$scene.png"
done
