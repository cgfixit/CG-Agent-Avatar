# CG-Agent-MacOS-Avatar

Menu-bar creature for Apple Silicon macOS. It talks to a **running** local
[CG-Agent-Harness](https://github.com/cgfixit/CG-agent-harness) and/or
[Ollama](https://ollama.com). It does not start either of them.

Ships as `CG-Agent-MacOS-Avatar.app` (bundle id `com.cgfixit.cg-agent`).
This GitHub repository is **private**.

## What it is

- A status-item extra: left-click the icon in the menu bar.
- A transparent strip at the top of the display with a walking avatar.
- A text field (typing works even if backends are down).
- A speech bubble **above** the avatar for replies.

## What it is not

- Not CG-Agent-Harness. Not a second copy of `CG Agent Harness.app`.
- Not an agent runner. It never calls `/api/agent/*`, never sends `"loop": true`,
  never reads `~/.CGagentHarness/.env`.
- Not a cloud client. Loopback IPv4/IPv6 only (`127.0.0.1` / `::1`). The name
  `localhost` is rejected.

## Menu

| Item | Effect |
|---|---|
| **Talk** | Show the strip and focus the field |
| **Harness (127.0.0.1:8790)** | Default. `POST /api/chat` with CSRF from `GET /` |
| **Direct Ollama (qwen3.8:27b-mlx)** | Override. `POST http://127.0.0.1:11434/v1/chat/completions` |
| **Quit CG-Agent-MacOS-Avatar** | Exit |

Click the creature for Talk. Return sends. While a turn is in flight the bubble
shows `…thinking`. If the chosen backend is down the field still types; the
bubble says `harness asleep` or `ollama asleep`.

## Harness port

Headless `cgagentharness serve` listens on **8790** (or `harness.json` `port`).

The bundled **CG Agent Harness.app** does **not**. Its sidecar binds
`127.0.0.1:0` (ephemeral). If `:8790` fails, this avatar looks up a
current-user `cgagentharness` LISTEN socket on `127.0.0.1` via argv-only `lsof`,
then checks `GET /api/status` looks like harness JSON. It does not scan the
port range and does not use the desktop focus socket.

You cannot run the desktop `.app` and `serve` on the same home at once (home lock).

## Build

Apple Silicon, macOS 13+.

```sh
./scripts/ci.sh
./scripts/package-app.sh
open dist/CG-Agent-MacOS-Avatar.app
```

The unsigned `.app` is **not** in git. Recipients build it. Ad-hoc `codesign`.
Gatekeeper may require Open anyway.

## Docs

- [docs/BUILD.md](docs/BUILD.md)
- [docs/CONTROLS.md](docs/CONTROLS.md)
- [SECURITY.md](SECURITY.md)

## License

MIT. See [LICENSE](LICENSE).
