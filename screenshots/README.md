# Screenshots (illustrations)

These six images are **synthetic illustrations, not native captures**. They were
rendered on Linux with headless Chromium from [`scenes.html`](scenes.html), which
reuses the real layout and color numbers from `src/theme.rs` (Classic and Fable
Protocol) and `OverlayLayout` in `src/app.rs`, plus the shipped
`assets/creature.png`. Fonts are Inter standing in for the system font; the
desktop, notes window, clock, and every reply are made-up sample data. No model
produced the replies.

| File | Scene |
|---|---|
| `hero.png` | Classic theme, a release-prep question answered next to a notes window |
| `thinking.png` | `…thinking`, backend label shows `· Thinking…`, Send disabled |
| `expanded.png` | **See Less** state with a long reply in the scroll pane |
| `fable.png` | Fable Protocol theme |
| `menu.png` | Menu-bar menu with Direct Ollama selected |
| `notice.png` | Over-limit draft notice (the draft stays in the field) |

They do not prove how the app renders on any macOS version. For real captures
with provenance, see [`docs/screenshots`](../docs/screenshots/README.md).

## Regenerate

```sh
./scripts/render-screenshots.sh   # needs Chromium/Chrome and Python Pillow
```

Edit `scenes.html` to change a scene, keeping its numbers in step with
`src/theme.rs`.
