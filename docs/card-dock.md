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
preferred side. The fallback does not overwrite the preference. When the window moves or shrinks
and the room appears, the card goes back. An inside preference stays inside.

**Presentation** (how it shows itself):

* `Pinned`: always open.
* `Bookmark`: a folded tab that unfolds while the pointer is on it.

When the player has chosen nothing (`presentation` absent in the settings), the default is `Pinned` beside
the game, which keeps the earlier behaviour, and `Bookmark` inside, because an open card would cover the game.
The default follows where the card really is, so a card that falls back inside a cramped screen becomes a tab
by itself. The pin button in the card's header (left of the power icon; tooltip "접기" while pinned, "고정"
while folded) sets the choice, and the settings keep it. With no game window the card waits at the top right of
the screen, always open, with no pin button.

## Geometry (`placement.rs`)

* `slot(frame, anchor, size)` returns the card's rectangle. Beside the game, the gap is 1.2% of the client
  height (at least 6 px), the top is level with the client top, and the card is pushed up to stay on the monitor.
  Inside, the margin is 1.2% of the client width (at least 4 px). If the card is too tall for the room, it
  is first pushed up (no higher than the margin below the client top) and then cut. The card does not
  scroll, so the part below the cut is not shown (the header and the first rows, which carry the
  verdicts, stay). A card wider than the client is narrowed to it.
* `resolve` returns the open rectangle plus the tab rectangle. The tab hangs on the card's edge nearest
  the anchor and shares the card's top. That edge is the left edge for `Outside(Right)` and `Inside(TopLeft)`,
  where the card opens to the right, and the right edge for the other two, where it opens to the left.
* Inside width: `inside_card_width` is 34% of the client width, clamped to 220 to 320 px. Beside the game
  it is 58%, clamped to 230 to 360 px. A 1280 px client gives 320, so the card covers at most a quarter of the game.
* Tab: 28 x 72 px at the card's design scale (card width / 300), at least 18 x 48 px.
* The slide (`strip`): the card comes out of its edge, and the part that has crossed it is shown. That is
  the card's right end when it grows to the right and its left end when it grows to the left, and the
  height grows with it. Only the part of the layer's own size is drawn, so the rest of the game
  stays clickable.

## Hover (`fold.rs`)

A state machine driven by a clock the caller passes in (ms); the tests pass a fake clock.

| Timing | Value |
|---|---|
| Dwell on the tab before it opens | 100 ms (a pass-by does not open it) |
| Slide | 120 ms, ease-out |
| Close delay after the pointer left | 350 ms |

If the pointer comes back while the card slides shut, the card reopens at once with no new dwell. "On it" means
the pointer is inside the current rectangle (tab, strip, or open card), so an open card stays open while the
pointer is on it. A tooltip needs the pointer on the card, so a showing tooltip already keeps it open.

* While the open card is dragged, it stays open. While the folded tab is dragged, it stays shut, so it never
  unfolds under the pointer.
* When the window is covered or hidden, the state resets to folded and nothing is animated unseen.
* When the card is pinned or there is no game, it is open and not animated.
* **Scan (input locked).** The scan sends real clicks to screen positions (`hcu` `SendInput`), and
  the card layer is not click-through, so an open card inside the game could swallow them. While
  the input is locked an inside card is therefore a folded tab that never unfolds and shows a green
  dot. In `crates/hcu/src/{roster,deep,scan}.rs` (1280 x 960 frame), the scan clicks and
  scrolls at back (50, 40), the arrow (1250, 505), the list scroll point (1150, 750), the student
  cards, and in the stat popup (755 to 1138, 258 to 320). The folded tab is about 28 x 72 px at
  the card's design scale, starts 7.5% down (72 px), and sits at x 15 to 43 or 1237 to 1265 at that
  size, so none of those points lie under it. A narrower window shrinks the tab with the card,
  and the 7.5% rule keeps it clear of the back button. Beside the game nothing changes, because the progress
  stays visible on the open card. If the input block fails to start, `locked` stays false and an
  inside card opens on hover during the scan. That is the same as no protection, because the unfolded card could
  then cover targets.
* **The click-through in-game card** (widgets set to 게임 안) sits at the same top right corner. When
  the main card is anchored at the inside top right, the overlay moves the in-game card left by the tab's
  width plus 6 px so the tab does not hide it. An unfolded card still covers it while the pointer
  is on it.

The overlay thread ticks every 90 ms normally, 40 ms while the card is a tab (hover), 16 ms while
it slides or is dragged. The thread skips a redraw when what is shown (content, place, shape) is unchanged.

## Magnet (`snap`)

The open card, dragged by its header, and the tab, dragged anywhere on it, follow the pointer. On release
`snap(frame, size, pointer, available)` picks the available anchor whose rectangle (for the size
of what is dragged) has its middle nearest the pointer. The available anchors are both inside corners, always, and each
side beside the game only if it has room for the card. While dragging, the overlay draws the nearest anchor's
rectangle as a faint outline (in the tooltip layer). A press that moves the pointer less
than 6 px (a click) chooses nothing, and neither does a release with nothing to snap to (the game
vanished mid-drag). Only a real move replaces the stored preference. A real move ends as a
choice even if it lands where the card already was (beside the game with no room, then dropped on
the inside corner, makes the inside corner the preference). The overlay reports the result as an event
and saves it:

* `dock:l`, `dock:r` (beside the game; the first version's events, unchanged), `dock:il`, `dock:ir`
  (inside the top left / top right corner);
* `present:pinned`, `present:bookmark` (the pin button).

## Differences from the first brief, on purpose

* The inside anchors sit 7.5% down the client (constant `INSIDE_TOP` in `placement.rs`), not at the
  client top, to stay below the game's currency and menu bar like the in-game card already did.
* The tab opens after a 100 ms dwell, so sweeping the pointer across the corner does not open it.
* The settings fields are `dock_anchor` and `presentation` (`placements` already means the widgets).
* `resolve(..)` takes the anchor from `effective_anchor(..)`, because the card's width (and so its
  height) depends on whether it is beside or inside the game.

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
