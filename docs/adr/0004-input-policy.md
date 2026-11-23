# 0004 Input only for the scan, guarded at one boundary, observe never intercept

Status: Accepted

## Context
Sending input to a live game is the risky part: wrong window, stale coordinates, half-sent actions, and a
player who needs control back. A PRD review proposed a general policy layer for agents.

## Decision
1. Passive code holds a read-only handle (`Hcu`). Input exists only on `Exclusive`, taken when the player
   starts a scan.
2. The native runner refuses input at its boundary: a point on no monitor, a changed window layout over the
   target, a foreign window under the point (the add-on's own windows are excluded from the layout), and
   partial delivery answers `effect_unknown` and is never retried.
3. Every input call is logged (time, points, window, result code) to `%APPDATA%\baocr\input.log`. No typed
   content, no images. `BAOCR_INPUT_LOG=0` turns it off.
4. During a scan the player's own input is blocked by low-level hooks that let injected events pass. An
   injected ESC does not abort, hardware ESC does. The block lifts at scan end, on ESC, from the tray, and
   after 40 minutes.
5. The add-on may observe the player's mouse (optional, off by default) but never intercepts or forwards it.

## Alternatives
- A full policy engine with modes and consent: too much for one game with two modes (read, scan).
- A transparent overlay that captures clicks and re-injects them: adds a hop, breaks drag, hold and scroll,
  and makes every play input look injected (account risk). Rejected.
- No input block: a stray click during a scan corrupts the data.

## Consequences
- ESC and the real input lock cannot be tested with injected events; a human checks them.
- Guards are tested with a fake desktop; the real paths are exercised by the live e2e harness.
- Measured cost of the settle wait after input: about 130 ms on a still screen in tests, up to 600 ms on a
  never-still one (`BAOCR_HCU_SETTLE_MS`).

## Evidence
`hcu/src/guard.rs`, `native.rs`, `audit.rs`, `overlay/src/input_block.rs`, `docs/hcu-native.md`.

## Revisit when
The game exposes an input API, or a scan must run unattended.
