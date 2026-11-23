# Live end-to-end harness

One command: `pwsh scripts/live-e2e.ps1 -Label directml` (and `-Label cpu` to run the same suite with `BAOCR_OCR_DEVICE=cpu`).
Code: `rust/crates/cli/examples/live_e2e/{main,rig,scenarios}.rs`, the hook `rust/crates/cli/src/dump.rs`.
Reports: `<out>/<label>/report.md|json`, screenshots in `<out>/<label>/shots`.

## The hook
`BAOCR_STATE_DUMP=<file>`: the add-on wraps its `Surface` in `DumpSurface` and rewrites the file atomically on every change and at
least every 250 ms: main card (scene, title, rows as text, close), in-game card, marks count, dock (anchor, presentation) as the overlay
holds it, card layer (`fold` open/folded/sliding/hidden, slide, rect, hit areas in screen pixels), `battle`, `locked`, `seq`, `mono_ms`,
`unix_ms`, `changed_unix_ms`. Unset: nothing is in the path. Pure parts are unit tested (`dump.rs`).

## Safety
Game input is navigation and the add-on's own scan only. Every click needs a fresh dump naming the expected scene and a frame that is
not a battle; `Rig::forbidden` refuses 출격 areas, 소탕, real raid 입장 and any click on a real 총력전 formation. The one 입장 pressed is the
모의 총력전 room popup (title must say 모의). 제약해제결전 is entered up to its 편성 screen only. Mouse input on the add-on's own card is real
input. `overlay.json` is restored from the backup at the end. `recover()` backs out to the 업무 hub.

## Scenarios and criteria
| id | what | criterion |
|---|---|---|
| nav | 업무, 총력전, 모의 boss list (5 bosses), 방 정보, 모의 편성, party tabs | scene label right; independent OCR has the screen's words; boss armor label from the DB is on the card; no "화면 / 위키" note; click-to-card median under 1.0 s |
| stale | 빠른 편성 popup x5 | icons appear; marks 0 within 1.0 s of closing; stay 0 across a forced card redraw |
| tour | 종합전술시험, 대결전, 현상수배, 특별의뢰, 학원교류회, 제약해제결전 (select, 단계, stage, 편성), main menu | scene label right, card rows |
| battle | detector on fixtures, plus every live frame grabbed | true only for battle.png; 0 live frames true |
| card | fold, pin, hover open/close, 40 ms pass, drag to anchors | open in 100..500 ms, close in 330..800 ms, anchors reached |
| scan | card button, non-deep, to the end, injected ESC ignored | count equal, unresolved 0, complete, fields equal except page/column/detail_path |
| guard | doctor, a covering window refuses the click, `input_selftest` | refused with `target_window_mismatch`, 0 events reach the cover, no FAIL |

## Not automatable (human)
Real ESC and the real input lock (hooks ignore injected events by design), tray 종료, how the card looks, feel of hover and drag,
game frame rate under DirectML, whether the unscripted game behaves the same on another monitor layout.

## Known limits of the harness
Latency is measured to the dump's `changed_unix_ms`. Other windows or agents that take focus or sit over the game make clicks and
hover results flaky (see the 2026-11-23 report in the session notes).
