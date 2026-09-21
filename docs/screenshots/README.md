# Native screenshots

These PNGs replace the README's externally hosted image. They are unedited
app-window captures taken through Computer Use (`@oai/sky`), with no desktop,
account details, credentials, or unrelated windows included.

| Property | Capture |
|---|---|
| Date | September 21, 2026 |
| Source revision | `7656bc3543bef66f881a2ad4672c30cb55a1e05d` (main at capture time) |
| Platform | Apple Silicon, macOS 27.0 (26A428) |
| Build | `./scripts/package-app.sh`, Rust 1.88, ad-hoc signed |
| Executable SHA-256 | `9b57de8d8d6123de099e1da5c517ca5111935014a3ae1d943ab3b52c8dad2618` |
| Theme/backend | Classic / live Direct Ollama, fixed model tag `qwen3.8:27b-mlx` |
| Harness home | Empty, isolated temporary directory |
| Prompt | “Give me eight numbered steps for a calm coding session, about 700 characters in plain text.” |

- [`classic-reply.png`](classic-reply.png): completed reply in the compact bubble.
- [`expanded-reply.png`](expanded-reply.png): the same reply after **See More**.

The reply is live model output, so wording varies between runs. These images
show native rendering and a working local chat request; they do not establish
Harness authentication, remote-provider behavior, notarization, or support for
every macOS version.

## Reproduce a capture

1. Run the [local checks and package validation](../BUILD.md), and record the
   source revision and executable hash before launching.
2. Launch that exact `.app` with an isolated `CGAGENTHARNESS_HOME`. Use public demo
   text and the intended backend. Avoid screenshots of login/password dialogs.
3. Click the creature, enter the prompt, verify the text field, and press Return.
   Confirm the thinking state and completed reply.
4. Capture only the app window. Expand with **See More**, capture the second
   view, then check **See Less** restores the preview. For a longer reply,
   also verify scrolling and text selection after several animation frames.
5. Review the pixels for legibility and private content, copy the original PNGs
   into this directory, and update the provenance table and README captions.

Do not describe fixture responses as live-provider results. Screenshots and
native interaction checks complement the automated tests; neither proves the
other passed.
