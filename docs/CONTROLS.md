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
| `search the web for …`, `look up …`, `google …` | Direct Ollama runs one web search first, then answers from the results. See [Web lookups](#web-lookups-direct-ollama) |
| `read <link>`, `summarize <link>` | Direct Ollama reads that one public page first, then answers from it |
| **See More** | Expand a reply into a scrollable pane; **See Less** collapses it |
| Harness/Ollama down | Field still types. Bubble says `harness asleep` / `ollama asleep` |
| Harness found, not logged in | Bubble says `login required — use Harness Login… in the menu` |
| Harness needs a password change | Bubble says `bootstrap password must be changed — use Harness Password Reset…` |
| Harness certificate changed | Bubble says `harness certificate changed — verify it, then re-check trust` |
| A reply used the web | Bubble appends a compact `[via web ×N]` note. For Harness, N counts Harness's own web tool calls. For Direct Ollama it is `×1`, the lookup you asked for. Avatar never fetches pages itself |

Chat is not streaming. While a turn is in flight the bubble says `…thinking`.
Direct Ollama sends only the current message plus the bundled system prompt,
and the results of one lookup when you ask for one. Harness uses a server-side session. The input accepts up to 32,768 characters
after trimming, rejects NUL characters, and leaves invalid input unsent.
This is a character limit, not a token-context setting.

Harness Login and Password Reset do not select a chat backend: choose
**Harness (127.0.0.1:8790)** before sending a Harness message. Wait for login
success before choosing Password Reset. The reset requires the current password;
the new-password prompt's **OK** submits the change. **Cancel** does not change
the password. Account sessions last only for the running Avatar process.

The overlay follows the creature but only covers the creature, bubble, and text field. Clicks elsewhere in that horizontal band go to the app underneath.

The initial reply bubble is 640×112 points under the default **Classic** theme. Expanded replies remain plain text and can scroll when longer than the available panel height. The preview caps at 400 characters and the expanded text at 8,000, with an ellipsis at the limit. Classic expansion caps at 420 points; Fable Protocol caps at 480.

## Web lookups (Direct Ollama)

A Direct Ollama message reaches the web only when it starts with one of these
phrases. Case doesn't matter, and a leading "please", "hey", "ok", "can you", or
"could you" is ignored.

| Start the message with | Lookup |
|---|---|
| `search the web for`, `search the internet for`, `search online for`, `search the web`, `search the internet`, `search online`, `web search for`, `web search`, `search for`, `look up`, `google` | One web search for the rest of the message (up to 5 results) |
| `read`, `open`, `fetch`, `summarize`, `summarise`, `visit`, followed by an `http://` or `https://` link anywhere in the message | Reads that one page |

Anything else, including a message that starts with just `search` or `research`,
stays a local chat. The lookup runs through the local Ollama service's
experimental web routes, which need Ollama 0.18.1 or newer and `ollama signin`.
Ollama sends the query or link to its cloud service under your account; see
[README](../README.md#web-lookups-direct-ollama) for the privacy boundary.

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
| `ollama: web lookup unavailable (http 404)…` | This Ollama predates the web routes. Update to 0.18.1 or newer. |
| `ollama: web lookup unavailable (http 401)…` or `(http 403)…` | Run `ollama signin`, and make sure Ollama's cloud features aren't disabled. |
| `ollama: can't read that link…` | Only public `http(s)` pages can be read. Refused: `localhost`, private or link-local IPs, `.local` names, single-word hosts, and links with embedded passwords. |
| A search was answered without looking anything up | Start the message with a lookup phrase from [Web lookups](#web-lookups-direct-ollama). Plain questions stay local. |
| `harness asleep` | Confirm Harness is running and Avatar uses the same home. The desktop sidecar's port may differ from the menu label. |
| Harness desktop app not found | Install/register the desktop app, or run a headless server for the selected home. |
| Login succeeds but a chat goes to Ollama | Select Harness explicitly; the login action does not change the backend. |
| Certificate mismatch | Verify Harness's certificate and home. Avatar does not offer a bypass or change Keychain trust. |
| Long response appears cut off | Expand with See More, then scroll; the expanded display still has an 8,000-character limit. |

For fresh native examples, see the [README screenshots](../README.md) and
[capture provenance](screenshots/README.md). For build errors, see
[BUILD.md](BUILD.md).
