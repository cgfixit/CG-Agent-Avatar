# Sprite plan

Status: **plan only, nothing here is shipped.** It specifies the frame set and
the drawing changes for an animated creature, and lists the decisions that are
still open. Delete this file once the work lands; `docs/CONTROLS.md` then owns
the behavior.

## Why

Today the creature is one PNG (`assets/creature.png`, 589 × 778, about 400 KB)
drawn whole on every frame.

- It never mirrors, so it walks backwards whenever it heads left.
- Moods change only its opacity (asleep, sick) and a vertical offset.
- While the chat panel is open the offset is forced to zero, so the creature
  is a still image for the whole conversation.

Replacing the art is also the moment to fix its likeness: the current sprite
reads as a mashup of two well-known film characters. An original creature
removes that risk from a public repository.

## Frame set

One sprite sheet, a grid of equal cells. Each row is one animation, each
column one frame. A second sheet (or a second block of rows) holds the same
frames facing left.

| Row | Animation | Frames (minimum → full) | Plays when | Loop |
|---|---|---|---|---|
| 0 | idle | 1 → 2 breathing, plus 1 blink | awake, standing still, panel open or closed | breath loops; blink every few seconds |
| 1 | walk | 2 → 4 | roaming with the panel closed | loops while moving |
| 2 | think | 1 → 3 | a reply is pending and no text has arrived | loops |
| 3 | talk | 2 → 3 mouth shapes | reply text is arriving, and for 8 s after a whole reply | loops |
| 4 | asleep | 1 → 2 | backend unreachable | slow loop |
| 5 | sick | 1 → 2 | backend up but not usable | slow loop |

Minimum is 8 frames per direction; the full set is 17.

**Cell size.** 144 × 192 px, a 3:4 cell. That is 2× the largest on-screen
height (96 pt in the Fable Protocol theme), so it stays sharp on Retina and
the sheet stays far smaller than today's single image. `creature_width` in
`src/app.rs` uses the old 589:778 ratio and changes to 3:4.

**Art rules.** Transparent background, feet on the same baseline in every
cell, the same head height in every cell (so frames do not jitter), and no
detail that vanishes at 88 pt. The menu-bar icon is a separate 18 pt
single-color template image, not a shrunken frame.

## Drawing changes

Put the decision in a portable module and keep AppKit to the draw call.

1. **New `src/sprite.rs` (layer 3, no AppKit).** A pure function picks the
   cell: `frame(mood, moving, facing, tick) -> (row, column)`. It also owns
   frame durations per animation. It is unit-tested on Linux like `mood.rs`.
2. **`CreatureView::draw_rect` (`src/app.rs`).** It already calls
   `drawInRect:fromRect:operation:fraction:` with an empty source rectangle,
   meaning "the whole image". Passing the cell's rectangle instead draws one
   frame. No new AppKit API is needed for frames.
3. **Mirroring.** Use pre-mirrored cells and pick the left-facing block when
   `Walk::dir` is negative. This needs no transform and no new `unsafe`. The
   alternative, flipping the graphics context at draw time, halves the art
   file but needs a transform API whose objc2 bindings have not been checked.
4. **State the view needs.** `Shared` gains what the view cannot derive:
   facing and whether the creature is moving. The tick already knows both.
5. **Redraw.** Today the view redraws only when the mood changes. It must
   also redraw when the chosen cell changes, and only then.
6. **Panel open.** Keep the panel still, but let idle, think, and talk frames
   play inside it. Reduce Motion (the setting and the macOS switch) holds the
   first frame of each animation and keeps the mood change.
7. **Opacity.** Once asleep and sick have their own frames, drop the opacity
   dimming, or keep a light one for asleep only.

## Verification

- `sprite.rs`: run on Linux, table of mood × moving × facing × tick → cell.
- `app.rs`: type-check for `aarch64-apple-darwin` from Linux; the real check
  is a Mac run with a native screen recording of each mood.
- A test asserts the sheet's pixel size is a whole number of cells.

## Open decisions

1. **Who makes the art, and what is the creature?** Everything else waits on
   this. An original design is the recommendation.
2. **Minimum set first (8 frames) or the full set (17)?** Recommendation: ship
   the minimum, since the code is the same and more frames drop in later.
3. **Pre-mirrored cells or a draw-time flip?** Recommendation: pre-mirrored,
   unless the creature is asymmetric on purpose (a cane in one hand reads
   fine either way; text on clothing would not).
4. **Per-theme sheets?** Recommendation: one sheet for both themes at first.

## Order of work

1. Art for the minimum set, both directions.
2. `sprite.rs` with tests, then the `draw_rect` change. One PR.
3. Animation while the panel is open, and the Reduce Motion behavior.
4. Remaining frames, then remove the unused images in `assets/`.
