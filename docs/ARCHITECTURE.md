# Architecture

The add-on is one Windows program (`baocr`, written in Rust) that sits beside the Blue Archive window,
reads what the game shows with OCR and pixel tests, and answers with a card and icons drawn over the game.
Reading the roster is a separate feature that the player starts by hand. Everything runs in one process.

An HTML version with the diagrams is in [architecture.html](architecture.html). Each decision has its own
record in [adr/](adr/README.md). This page is the map.

## What it does

| Feature | Input to the game | Where it lives |
|---|---|---|
| Passive guide: scene label, mission and raid card, squad verdict, portrait and slot icons | none (screenshots only) | `assist`, `overlay`, `extract` |
| Student roster scan (JSON output) | clicks and scrolls, only after the player pressed the button | `hcu`, `cli` |
| Screen learning (`probe`, `--learn`) for new fixtures | none or navigation clicks by the developer | `cli/examples` |

## Runtime flow

```
 game window ──► capture ──► settle check ──► OCR / pixel tests ──► scene + keys ──► Observation
   (Win32)       (hcu)       (frame key)       (ocr, extract,         (extract)         │
                                                pixels)                                  ▼
 overlay thread ◄── Surface ◄── Assistant ◄── widgets (wikidb lookups, matchup, weights)
 (layered windows)  (trait)     (diffing)       static DB data

 player ──► card buttons ──► events ──► Assistant ──► settings / commands (scan, quit)
```

1. The loop (`cli/src/assist.rs`) captures the game client as a 1280x960 frame, tells whether the view is
   moving or settled with a cheap frame key, and detects a battle from a few pixels.
2. OCR runs once on a settled view the loop has not seen. `extract` classifies the scene from header and anchor
   words, and trackers (`MissionTracker`, `RaidTracker`) remember what the player picked, because the
   later screens do not show it.
3. The `Observer` turns the frame into an `Observation`: scene, mission or raid key, squad, grid cards.
4. The `Assistant` asks every `Widget` what it would show, looks the facts up in the static wiki DB, and
   sends only changed outputs to the `Surface`.
5. The `Overlay` implements `Surface`. Its own thread draws the card, the in-game card, the icons and
   tooltips as layered windows, and reports button clicks back as events.

## Static and dynamic data

The screen only supplies keys. Facts come from the database ([ADR 0005](adr/0005-static-db-dynamic-screen.md)).

| Data | Kind | Source |
|---|---|---|
| Which screen, which mission, stage, raid boss, difficulty, mode | dynamic | OCR and pixels |
| Which students are in the squad, their slots | dynamic | OCR of names, closed-set reading |
| Terrain tab, selection state | dynamic (when readable) | pixels |
| Enemy armor and attack mix, levels, terrain of a stage | static | `wikidb/wiki.db` |
| Student attack and armor type, terrain adaptation, role | static | `wikidb/wiki.db` |
| Type effectiveness table, enemy weights | static | `wikidb/wiki.db` |
| Settings, dock placement, fold state | user state | `%APPDATA%\baocr\overlay.json` |
| Scan output | produced data | `%APPDATA%\baocr\students` |

## Modules

![crate dependency graph](diagrams/crates.svg)

The graph is generated from `cargo metadata` by `python scripts/gen_arch.py`. The Mermaid form is
[diagrams/crates.mmd](diagrams/crates.mmd).

| Crate | Responsibility | Notes |
|---|---|---|
| `baocr` (cli) | the add-on loop, the scan driver, `--doctor`, examples for learning and measuring | the only binary users run |
| `assist` | `Observer`, `Widget`s, `Assistant`, settings, raid lookup, matchup words | no drawing, no game access; `Surface` is the output port |
| `overlay` | layered windows, panel and icon rendering, placement and fold, tray, input hooks | Win32 only here and in `hcu` |
| `hcu` | desktop control in process: capture, zoom, click, scroll, window list; guards; scan loops; doctor | [hcu-native.md](hcu-native.md) |
| `extract` | profile reading, scene classification, mission and raid trackers, card names | pure functions over boxes and frames |
| `ocr` | PP-OCR detection and recognition on ONNX Runtime, closed-set CTC scoring | [ADR 0002](adr/0002-ocr-stack.md) |
| `pixels` | pixel classifiers, frame key, battle detector, icon tests | no OCR |
| `wikidb` | SchaleDB collector, schema, lookups, formation evaluation, raid data | [raid-data.md](raid-data.md) |
| `pipeline`, `export`, `names`, `equipment`, `core` | roster scan back end: async extraction, JSON store, name dictionary, equipment icons, shared model | |

Dependency rule: `cli` depends on `assist`, and `assist` on `overlay`, `extract` and `wikidb`. `hcu` depends on
`extract`, and `extract` on `ocr` and `pixels`.
Nothing below `assist` knows about drawing, and `overlay` knows nothing about the game's data.

## Threads and windows

| Thread | Job | Never does |
|---|---|---|
| main loop (`cli`) | capture, read, observe, present, run the scan | draw |
| overlay worker (`overlay/win.rs`) | draws layers on change, polls the cursor for hover, reports clicks | read the screen |
| tray (`overlay/tray.rs`) | notification icon and its 종료 menu on its own hidden window | depend on the other windows |
| input block (`overlay/input_block.rs`) | low-level hooks while a scan runs, ESC aborts | run outside a scan |
| input hints (`overlay/hints.rs`) | optional pass-through mouse hook (off by default, not wired) | block or forward input |

The card is an interactive layered window. The in-game card, the icons and the tooltip are click-through.
All four are excluded from screen capture, so the add-on never reads itself.

## Constraints that shape the design

- The add-on reads the game only at 4:3 windowed, and the regions of interest are calibrated on a 1280x960 client.
- Input is sent only for the scan, behind guards ([ADR 0004](adr/0004-input-policy.md)).
- The add-on reads no game memory or files, only screenshots and public data.
- Build limits protect RAM and disk ([ADR 0009](adr/0009-build-limits.md)).

## Verification

| Level | What | Where |
|---|---|---|
| unit | pure logic: scenes, trackers, geometry, hover, weights, guards (fake desktop) | `cargo test --workspace` |
| fixtures | real screenshots and OCR boxes | `fixtures/screens` |
| measurement | timelines and OCR timing | `docs/speed-research.md`, `cli/examples/speed_*` |
| live e2e | the add-on on the real game, state dump | [live-e2e.md](live-e2e.md) when present |
| human | taste, hardware ESC, tray menu, frame rate | listed in the release check |

## Decision records

The earlier extraction-era notes (Go and Tesseract, the OCR benchmark, bond rank, risks) are kept in
[history/go-era-architecture.md](history/go-era-architecture.md).

[adr/README.md](adr/README.md) lists them. Detail notes: [hcu-native.md](hcu-native.md),
[raid-data.md](raid-data.md), [card-dock.md](card-dock.md), [speed-research.md](speed-research.md),
[enemy-weighting.md](enemy-weighting.md).
