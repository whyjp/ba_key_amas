# The card's place: anchors, bookmark tab, magnet

Code: `rust/crates/overlay/src/{dock,placement,fold,tab,win}.rs`, settings in `assist/src/settings.rs`.

## Model

Two independent choices.

**Anchor** (where the card lives), a preference:

| Anchor | Place | Opens toward |
|---|---|---|
| `Outside(Right)` / `Outside(Left)` | against the game window, top level with the game's client top | away from the game |
| `Inside(TopRight)` / `Inside(TopLeft)` | inside the client area, at that top corner, below the game's own header bar (7.5% of the client height, like the existing in-game card) | toward the middle of the game |

Beside the game only works when the work area of the game's monitor has room for the card there.
`effective_anchor` is the preferred anchor, else the other side, else the inside corner on the
preferred side. The preference is not overwritten by the fallback: when the window moves or shrinks
and the room appears, the card goes back. An inside preference stays inside.

**Presentation** (how it shows itself):

* `Pinned`: always open.
* `Bookmark`: a folded tab; it unfolds while the pointer is on it.

Default (nothing chosen, `presentation` absent in the settings): `Pinned` when the card is beside
the game (today's behaviour, nothing changes for the player), `Bookmark` when it is inside (an open
card would cover the game). The default follows where the card really is, so a card that falls
back inside a cramped screen becomes a tab by itself. The pin button in the card's header
(left of the power icon; tooltip "접기" while pinned, "고정" while folded) sets the choice and it is
kept. With no game window the card waits at the top right of the screen, always open, no pin button.

## Geometry (`placement.rs`)

* `slot(frame, anchor, size)`: the card's rectangle. Beside the game: gap 1.2% of the client
  height (at least 6 px), top level with the client top, pushed up to stay on the monitor.
  Inside: margin 1.2% of the client width (at least 4 px); if the card is too tall for the room it
  is first pushed up (no higher than the margin below the client top), then cut. The card does not
  scroll; the part below the cut is not shown (the header and the first rows, which carry the
  verdicts, stay). Wider than the client: narrowed to it.
* `resolve`: the open rectangle plus the tab rectangle. The tab hangs on the card's edge nearest
  the anchor (left edge for `Outside(Right)` and `Inside(TopLeft)`, the card opens to the right;
  right edge for the other two, it opens to the left) and shares the card's top.
* Inside width: `inside_card_width` = 34% of the client width, 220 to 320 px (beside the game:
  58%, 230 to 360 px). 1280 px client gives 320; the card covers a quarter of the game at most.
* Tab: 28 x 72 px at the card's design scale (card width / 300), at least 18 x 48 px.
* The slide (`strip`): the card comes out of its edge, so the part that has crossed it is shown:
  the card's right end when it grows to the right, its left end when it grows to the left; the
  height grows with it. Only the part of the layer's own size is drawn, the rest of the game
  stays clickable.

## Hover (`fold.rs`)

State machine on a caller clock (ms), tested with a fake one.

| Timing | Value |
|---|---|
| Dwell on the tab before it opens | 100 ms (a pass-by does not open it) |
| Slide | 120 ms, ease-out |
| Close delay after the pointer left | 350 ms |

While it slides shut, coming back catches it at once (no new dwell). "On it" means the pointer is
inside the current rectangle (tab, strip, or open card), so the open card keeps itself open.
A tooltip needs the pointer on the card, so a showing tooltip keeps it open already.

* Dragging the open card: held open. Dragging the folded tab: held shut, so it never unfolds
  under the pointer.
* Covered or hidden window: the state is reset to folded, nothing is animated unseen.
* Pinned (or no game): open, no animation.
* **Scan (input locked).** The scan sends its clicks to screen positions, and the card layer is
  not click-through. An open card inside the game could swallow them, so while the input is
  locked an inside card is a folded tab, never unfolds, and shows a green dot. Beside the game
  nothing changes (the progress stays visible on the open card).

The overlay thread ticks every 90 ms normally, 40 ms while the card is a tab (hover), 16 ms while
it slides or is dragged. Redraws are skipped when what is shown (content, place, shape) is unchanged.

## Magnet (`snap`)

Dragging the open card by its header, or the tab anywhere on it, follows the pointer. On release
`snap(frame, size, pointer, available)` picks the available anchor whose rectangle (for the size
of what is dragged) has its middle nearest the pointer. Available: both inside corners always, each
side beside the game only if it has room for the card. While dragging, the nearest anchor's
rectangle is drawn as a faint outline (in the tooltip layer). The result is reported as an event
and saved:

* `dock:l`, `dock:r` (beside the game; the first version's events, unchanged), `dock:il`, `dock:ir`
  (inside the top left / top right corner);
* `present:pinned`, `present:bookmark` (the pin button).

## Settings (`%APPDATA%\baocr\overlay.json`)

* `dock_right` (old, still written for the beside-the-game side).
* `dock_anchor`: `"l" | "r" | "il" | "ir"`, optional; overrides `dock_right` when valid.
* `presentation`: `"pinned" | "bookmark"`, optional; absent means the default above.

Old files (without the two fields, or with unknown values) load and behave as before.

## What was not verified live

Verified: pure tests (geometry for a roomy QHD window, a full 1080p game, a window at the monitor
edge, tiny work area; snapping; hover with a fake clock; settings backward compatibility; press
order tab / header icons / drag handle) and preview images over `fixtures/screens` (read only).
Not verified, needs the real game window:

* Hover timing feel (100 / 120 / 350 ms) and the 40 ms poll; whether the tab at the inside
  corner gets in the way of the game's own top-right buttons (it sits below the header bar by design).
* Z-order of the tab and the card against the game (exclusive full screen would hide all of it,
  as before), and against the tooltip and snap-hint layer (the hint is drawn on the tooltip layer).
* Per-monitor DPI: the loop calls `SetProcessDPIAware`, all sizes derive from the client's pixels;
  moving the window between monitors of different scale was not tried.
* A real scan with an inside card: that the scan's clicks reach the game while the inside card is
  folded (the design says they do; no input was sent in this work).
* Whether a drag by the tab feels right at the game's edge, and whether leaving the pointer on the
  tab while a window above the game appears behaves (the `covered()` path hides everything).
* The layer is recreated at the size of the tab and of every slide step (an allocation per frame
  for 120 ms); not measured.
