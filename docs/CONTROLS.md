# Controls

Left-click the menu-bar icon (top toolbar). Typing does **not** require Harness or Ollama; those are only needed for a model reply.

| Input | Action |
|---|---|
| **Talk** | Show the strip, focus the text field (works offline) |
| **Harness (127.0.0.1:8790)** | Optional. Tries `:8790`, then finds the desktop `.app` sidecar on `127.0.0.1`; HTTPS with a pinned certificate on fresh homes, plain HTTP on legacy homes |
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

Harness Login and Password Reset do not select a chat backend: choose
**Harness (127.0.0.1:8790)** before sending a Harness message. Wait for login
success before choosing Password Reset. The reset requires the current password;
the new-password prompt's **OK** submits the change. **Cancel** does not change
the password. Account sessions last only for the running Avatar process.

The overlay follows the creature but only covers the creature, bubble, and text field. Clicks elsewhere in that horizontal band go to the app underneath.

The initial reply bubble is 640×112 points under the default **Classic** theme. Expanded replies remain plain text and can scroll when longer than the available panel height.

## Design system

All layout sizes, motion (walk speed, bob, frame rate), and bubble color/type come from a `Theme` in `src/theme.rs`, chosen once at launch via `CG_AGENT_THEME` (case-insensitive):

| Value | Look |
|---|---|
| `classic` (default) | A translucent-white bubble with fixed dark reply text, readable in either system Appearance, at 30fps |
| `fable-protocol` (or `fable`) | A second design system: a larger stage, a fixed dark-ink bubble with warm parchment text (does not follow system Appearance), and a calmer 24fps gait |

```sh
CG_AGENT_THEME=fable-protocol ./dist/CG-Agent-MacOS-Avatar.app/Contents/MacOS/cg-agent
```
