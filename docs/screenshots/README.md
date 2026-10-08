# Native screenshots

These JPEGs show the updated chat panel. They are unedited
app-window captures taken through Computer Use (`cua_repl`), with no desktop,
account details, credentials, or unrelated windows included.

| Property | Capture |
|---|---|
| Date | October 8, 2026 |
| Source revision | `e19534ed834a403c54f715e596362d0f767c163c` |
| Platform | Apple Silicon, macOS 27.0.1 |
| Build | `./scripts/package-app.sh`, Rust 1.88 toolchain, ad-hoc signed |
| Executable SHA-256 | `e9d55852b0ca6da3f13dc85612f9b4133d3b0252bc60ef3b95cd7244a39d3031` |
| Theme/backend | Classic / live Direct Ollama, fixed model tag `qwen3.8:27b-mlx` |
| Harness home | Empty, isolated temporary directory |
| Prompts | Three small ways to start a project (under 45 words); an 18-step debugging checklist with two short sentences per step |

- [`classic-reply.jpg`](classic-reply.jpg): completed reply with backend context, separate reply controls, and the wider composer.
- [`expanded-reply.jpg`](expanded-reply.jpg): a long reply after **See More**, wrapped within the scroll view's content width.

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
5. Review the pixels for legibility and private content, copy the original captures
   into this directory, and update the provenance table and README captions.

Do not describe fixture responses as live-provider results. Screenshots and
native interaction checks complement the automated tests; neither proves the
other passed.
