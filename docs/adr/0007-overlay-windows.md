# 0007 Layered click-through windows, change cache, bookmark card

Status: Accepted. Details: [card-dock.md](../card-dock.md).

## Context
The overlay must not take focus, swallow the game's clicks, appear in the frames the add-on reads, or cost
CPU while nothing changes. Not every monitor has room beside the game.

## Decision
- Four layered windows with `WS_EX_NOACTIVATE`, topmost, excluded from capture (`WDA_EXCLUDEFROMCAPTURE`):
  the card (interactive), the in-game card, the icons layer and the tooltip (click-through).
- The overlay thread draws only on change: a change token per layer (`cache.rs`), memoised renders, and a
  wake signal instead of a fixed poll. Measured idle load fell from 11 to 14 percent of a core to 0.4 percent.
- The assistant sends the surface only outputs that changed. Icons stay down while the view is moving,
  because a redraw from a stale reading would put the old view's icons back.
- Card placement is one of four anchors (outside left or right, inside top left or right) with a presentation,
  pinned or bookmark. Outside needs room on the monitor, otherwise the card falls back inside, and the stored
  preference is kept. The bookmark tab is a small window, so the rest of the game stays clickable. Hover
  opens after 100 ms, slides in 120 ms, closes 350 ms after the pointer leaves. Dragging snaps to the nearest
  anchor.
- Closing is a power icon in the header, and the tray icon closes the add-on whatever the windows do.

## Alternatives
- One full-screen transparent window: simpler, but it covers every click and costs more to draw.
- Fixed polling of state: wasted wakeups and up to 90 ms added latency.

## Consequences
- The card width is 58 percent of the game window (230 to 360 px); inside it is 34 percent (220 to 320 px).
- Z-order, DPI per monitor and the feel of hover need a live check.

## Evidence
`overlay/src/cache.rs`, `placement.rs`, `fold.rs`, `tab.rs`, `win.rs`; tests with a fake clock; previews.

## Revisit when
The game ships its own overlay layer or changes window handling.
