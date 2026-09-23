# Controls

Left-click the menu-bar icon (top toolbar). Typing does **not** require Harness or Ollama; those are only needed for a model reply.

| Input | Action |
|---|---|
| **Talk** | Show the strip, focus the text field (works offline) |
| **Harness (127.0.0.1:8790)** | Optional. Launches the bundled `CG Agent Harness.app` (by bundle identifier) if it isn't already running, then tries the configured port (8790 by default) and finds the desktop `.app` sidecar on `127.0.0.1`; HTTPS with a pinned certificate on fresh homes, plain HTTP on legacy homes |
| **Direct Ollama (qwen3.8:27b-mlx)** | Default. Sends the bundled Soul prompt as a system message via `http://127.0.0.1:11434/v1/chat/completions` |
| **Harness Login…** | Prompts for a harness account username and password (native secure text entry), then logs in for later Harness chats |
| **Harness Password Reset…** | After logging in with a bootstrap password, securely replaces only that authenticated account's password |
| **Quit CG-Agent-MacOS-Avatar** | Terminate |
| Click the creature | Same as Talk |
| Type, Return | Send. Reply appears in the bubble **above** the avatar |
| **See More** | Expand a reply into a scrollable pane; **See Less** collapses it |
| Harness/Ollama down | Field still types. Bubble says `harness asleep` / `ollama asleep` |
| Harness found, not logged in | Bubble says `login required — use Harness Login… in the menu` |
| Harness needs a password change | Bubble says `bootstrap password must be changed — use Harness Password Reset…` |
| Harness certificate changed | Bubble says `harness certificate changed — verify it, then re-check trust` |
| Harness reply used web search | Bubble appends a compact `[via web ×N]` note — this app only displays that, it never fetches pages itself |

Chat is not streaming. While a turn is in flight the bubble says `…thinking`.
Direct Ollama sends only the current message plus the bundled system prompt;
Harness uses a server-side session. The input accepts up to 32,768 characters
after trimming, rejects NUL characters, and leaves invalid input unsent.
This is a character limit, not a token-context setting.

Harness Login and Password Reset do not select a chat backend: choose
**Harness (127.0.0.1:8790)** before sending a Harness message. Wait for login
success before choosing Password Reset. The reset requires the current password;
the new-password prompt's **OK** submits the change. **Cancel** does not change
the password. Account sessions last only for the running Avatar process.

The overlay follows the creature but only covers the creature, bubble, and text field. Clicks elsewhere in that horizontal band go to the app underneath.

The initial reply bubble is 640×112 points under the default **Classic** theme. Expanded replies remain plain text and can scroll when longer than the available panel height. The preview caps at 400 characters and the expanded text at 8,000, with an ellipsis at the limit. Classic expansion caps at 420 points; Fable Protocol caps at 480.

## Design system

All layout sizes, motion (walk speed, bob, frame rate), and bubble color/type come from a `Theme` in `src/theme.rs`, chosen once at launch via `CG_AGENT_THEME` (case-insensitive):

| Value | Look |
|---|---|
| `classic` (default) | A translucent-white bubble with fixed dark reply text, readable in either system Appearance, at 30fps |
| `fable-protocol` (or `fable`) | A second design system: a larger stage, a fixed dark-ink bubble with warm parchment text (does not follow system Appearance), and a calmer 24fps gait |

```sh
CG_AGENT_THEME=fable-protocol ./dist/CG-Agent-MacOS-Avatar.app/Contents/MacOS/cg-agent
```

## Troubleshooting

| Observation | Check |
|---|---|
| `ollama asleep` | Start the local service on `127.0.0.1:11434`; Avatar does not launch it. Confirm it lists the exact fixed model tag. |
| `pull qwen3.8:27b-mlx` | The service is running, but its model list has no entry named exactly `qwen3.8:27b-mlx`. Other `qwen3.8:27b` variants don't count. Install that tag in the service; the model is not configurable in the menu. |
| `ollama: ollama http 404` | The model is listed, but the chat request was rejected. Confirm that the service exposes the OpenAI-compatible `/v1/chat/completions` endpoint. |
| `harness asleep` | Confirm Harness is running and Avatar uses the same home. The desktop sidecar's port may differ from the menu label. |
| Harness desktop app not found | Install/register the desktop app, or run a headless server for the selected home. |
| Login succeeds but a chat goes to Ollama | Select Harness explicitly; the login action does not change the backend. |
| Certificate mismatch | Verify Harness's certificate and home. Avatar does not offer a bypass or change Keychain trust. |
| Long response appears cut off | Expand with See More, then scroll; the expanded display still has an 8,000-character limit. |

For fresh native examples, see the [README screenshots](../README.md) and
[capture provenance](screenshots/README.md). For build errors, see
[BUILD.md](BUILD.md).
