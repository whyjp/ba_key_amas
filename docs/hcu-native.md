# In-process desktop control (`crates/hcu`)

The add-on no longer starts Microsoft's `computer-use` helper. `native.rs` answers the same requests with the
same JSON records, so everything above the `Runner` trait is unchanged. The helper backend is still reachable
with `BAOCR_HCU=process` for comparison, until a live parity scan has been accepted.

Reference: Microsoft Hybrid Computer Use (MIT), revision `8ce82b310b014ab1133b28febc2eca81527e43e6`
(2026-11-18). Attribution: `THIRD_PARTY_NOTICES.md`. We read the C# as a specification and copied nothing.

## What was ported

| Request (CLI) | Native behaviour |
|---|---|
| `gui screenshot` | The whole virtual desktop, delivered at no more than 1366x768 and never enlarged (`delivered_size`). The runner reads the layout of the visible windows before and after the grab and repeats the grab (3 attempts, 25 ms apart) when the layout changed. The answer carries the delivered size. |
| `gui zoom x0 y0 x1 y1` | Region in delivered-frame pixels, at least 8 px a side, inside the frame. It snaps outward to whole native pixels, is enlarged at most 4x and at most to 1366x768, and the answer carries the region actually covered with its scale (`zoom_geometry`). |
| `gui click x y` and `gui --json` (click, scroll) | A frame point maps to the native pixel `(u + 0.5) * scale - 0.5` (clamped inside the desktop), is sent with `SendInput` as an absolute coordinate on the virtual desktop (`(p * 65536 + len - 1) / len`), then press and release of the left button. A scroll is moved-to, then wheel notches of 120 units, 12 ms apart. |
| `apps`, `activate` | Visible top-level windows with a title (`wN "title" app=window [active]`); activation restores a minimised window and takes the foreground by attaching to the foreground thread's input queue. |

Coordinates are physical pixels (the process calls `SetProcessDPIAware`). Frames are the whole desktop, so a
point in the game's client area is converted with `to_frame_region`.

## Not ported, on purpose

- Accessibility tree and UI Automation (`AX`, FlaUI). The add-on reads pixels only.
- Text input, key chords, drag, right/middle/double click: nothing here sends them.
- The helper daemon, its socket protocol, `hello`/`schema`/session handling, request replay, and MCP.
- The helper's final screenshot after each input call (the settle wait below replaces what callers used from it).
- Multi-monitor frame ids and the published-layout table of the C# (a frame here carries its own layout snapshot).

## Guards at the boundary

All of these live in `NativeRunner`, not in callers. They are refusals. Nothing is sent, and the answer is
`ok:false` with `input_emitted:false` unless stated.

1. **Layout per frame.** Each screenshot stores the visible top-level windows (handle, rectangle, class, front-to-back
   order; titles are ignored). The runner refuses input with `gui_frame_stale` when the windows over the target pixel are not
   the ones the frame saw (port of `WindowsDesktopZoom.RegionStillValid`). A zoom runs the same check on its region
   before and after the grab. `Hcu` retries an input once after a fresh screenshot, and only when nothing was sent.
2. **Window under the point.** Right before each action, the top-level window that receives a click at the pixel
   (`WindowFromPoint`, root) must have the exact title of the target (`Blue Archive` in the app). Otherwise
   `target_window_mismatch`.
3. **Own windows.** The add-on's own windows are left out of layout snapshots (they redraw constantly and would make
   every frame stale). They are click-through, so they cannot be the target either, and the runner refuses input
   when one is reported under the point. We decided this rather than assuming it, because a click that would land on
   an overlay is not aimed at the game. Click-through windows of other programs (layered and transparent) are
   skipped in snapshots for the same reason.
4. **Dead space.** The virtual desktop can contain pixels no monitor shows. A point there is refused with
   `target_off_monitor` (`MonitorFromPoint`, no default monitor). `baocr --doctor` on the development machine reports
   9 % dead space.
5. **Partial input.** When some events went out and a later one failed or was refused, the answer is `ok:false`,
   `code:"effect_unknown"`, `input_emitted:true`, `completed:n`. `Hcu` never repeats such a call. A failed press/release
   retries the release up to three times so the button is not left down.
6. **Audit log.** One JSON line per input call in `%APPDATA%\baocr\input.log` (UTC time, kind, number of actions, frame
   point, native pixel, title and class of the window under the point, result code, `input_emitted`, completed count,
   milliseconds). No typed content and no images. At about 1 MB the file becomes `input.log.1` (one old file kept).
   `BAOCR_INPUT_LOG=0` switches it off. A log that cannot be written does not stop the input.
7. **Settle.** After input the call waits until two captures of the target window, 60 ms apart, are identical, or
   `BAOCR_HCU_SETTLE_MS` (default 600; 0 turns the wait off) has passed. Measured with the fake desktop (a small window): about
   130 ms when the screen is still, about 650 ms when it never is. A game with an idle animation will often run to the
   cap, so expect up to 0.6 s per input call; the old helper took about 1.35 s per call. Not measured on the real game.

Passive code cannot send input: `Hcu` has screenshot, zoom, window list and frame size only. Input
(`click`, `scroll`, `scroll_n`, `actions`, `activate`) is on `Exclusive`, which only `HcuSession::exclusive()` creates.

## Known limits

- The game region is detected from the window rectangle once per scan. If the game window is moved mid-scan, the
  frame check refuses, `Hcu` re-takes a screenshot and repeats, and the same region then points at where the window
  used to be. The guard still checks that the window under the point is the game, but not that the point is the same place in it.
- A game running elevated while the add-on is not drops `SendInput` silently. `--doctor` detects it.
- `input_selftest` (examples) is the manual check against a harmless window; it is not run by `cargo test`.

## Activating the game window

`activate` asks Windows for the foreground with a plain `SetForegroundWindow` and waits up to 750 ms. It sends no key (an earlier version tapped Alt) and attaches no input queue, because those tricks take the focus from whatever the player is doing. If Windows refuses, the call fails with a message asking the player to click the game window once, and a scan does not start. Already being the foreground window is a success without any call.

## Trying the input path without the game

`baocr --doctor` reads the game window, DPI, monitors (including dead space in the virtual desktop), the
game's and the add-on's elevation and the focus, and prints OK/경고 lines. It sends no input.

Two examples in `rust/crates/hcu/examples` exercise the in-process input against a harmless window instead
of the game. They are **not** run by `cargo test`, because `input_selftest` moves the real mouse:

```
cargo build -p baocr-hcu --examples
cargo run -p baocr-hcu --example input_selftest     # starts input_target itself
```

`input_target` is a plain window titled "HCU Test Target" (coloured quadrants, a white marker square) that
logs the screen position of every click and wheel turn it receives. `input_selftest` takes a screenshot and
a zoom, clicks at three points, turns the wheel, and checks that the target saw each event where it was aimed.
It also checks the refusals, which must reach nothing: the window moved after the frame, the point covered by
another window, and a point on no monitor. Each check prints PASS/FAIL and the exit code is 1 on any FAIL.
Every input call is also written to `%APPDATA%\baocr\input.log` (the selftest uses `%TEMP%\hcu-selftest-input.log`).
