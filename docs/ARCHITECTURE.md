# Architecture and decisions

## Goals

1. Collect what the player sees on the student screens without retyping it.
2. View it as HTML.
3. (Next) show extra stats as an overlay in other in-game menus (squad
   composition etc.) without opening the student menu.

## Layers (extraction is independent of computer-use automation)

```
 FrameSource ──► Extractor ──► Store ──► Views
  (frames)       (pixels→data)  (JSON+history)  HTML now / overlay later
```

| Layer | Code | Notes |
|---|---|---|
| Frame sources | `arrow_scan.go` + `deepcapture.go` (HCU clicks), `watch.go` `hcuSource` (passive screenshots, **no input**), saved frames (`--reextract`, tests) | Anything that yields a 1280x960 (4:3) frame can feed the rest. |
| Extractor | `main.go extractWith`, `meta.go`, `deepcapture.go extractDeep`, `fixed.go` | Pure: image in, `Student` out. No game access. |
| Async pipeline | `pipeline.go` | Capture never waits for OCR; `index.json` tracks captured/extracted/failed; `--reextract` retries from saved frames. |
| Store | `store.go` | Latest record per student + `history.jsonl`; a field a blurry read could not determine never overwrites a known value. |
| Autonomous updater | `watch.go` (`--watch`) | Reads a profile only after it holds still, stores only what changed. Works while the player plays; no scan needed. |

Collection stages: **fixed meta** comes from the dictionary (only changes with
patches), **variable** values are read from pixels, a **single update** re-reads
one profile (see README "Collection stages").

## OCR engine decision (measured, not assumed)

Language does not change accuracy; the model does. So the question is
Tesseract (LSTM, 2018) vs PP-OCRv5 (what oar-ocr/RapidOCR run), not Go vs Rust.

Measured on 11 labelled screens / 68 fields (names, 4 stats, 4 skill levels,
exp, level strip, bond number, 12 popup rows); `tools/ocr-bench/bench.py`:

| Engine / setup | Correct |
|---|---|
| Tesseract, **heavily tuned** on these very screens (ink isolation, whitelists, dictionary snap) | 66/68 |
| PP-OCRv5 Korean, detection+recognition, **untuned** (padding only) | 64/68 |
| PP-OCRv5 Korean, recognition only on tight crops | 53/68 |
| PP-OCRv5 Korean, recognition only on padded crops | 51/68 |

Reading the numbers honestly:

- The Tesseract score is optimistic (tuned on the test set). PP-OCRv5 reaches
  parity with no tuning and reads the two cases Tesseract cannot: the thin
  italic `Lv.1` (2/2) and the bond number in the heart (8/8, no special code).
- Its misses were label bleed (`방어력 86` -> `886`) and `슌` -> `순`; both are
  fixable by parsing and the existing dictionary snap.
- Recognition-only is fragile (it wants a properly shaped text line);
  detection+recognition is robust but ~0.5 s/crop in Python.
- 68 fields on 11 screens is a small sample. A held-out comparison on the full
  roster (the scan data) is the next step before any switch.

Recommendation:

1. Keep Go. Rewriting in Rust buys no accuracy; Rust's advantage would be
   oar-ocr already implementing detection/recognition, not speed or accuracy.
2. Put a `Recognizer` seam behind `recognizePNG` (the single funnel every OCR
   call goes through) and add a PP-OCRv5 backend (onnxruntime in-process, or an
   OCR sidecar over local HTTP). Run both engines per field and use the
   existing cross-checks (panel vs popup, dictionary, locks) to arbitrate.
3. Decide the default engine on held-out data, field by field.

### Correction and frame-level measurement

What was actually measured (RapidOCR 1.32 / onnxruntime on CPU): detection
**PP-OCRv6 det small** (RapidOCR default), recognition **korean PP-OCRv5
mobile** (forced), angle classifier v2.0 mobile. The 64/68 above ran detection
+ recognition *per crop* (~0.5 s each, ~13 s per profile), which is too slow
for `--watch` or an overlay. Running detection once per frame/panel instead
(`tools/ocr-bench/frame_bench.py`):

| Setup | numbers (stats, skills, exp, level strip: 37 fields) | names (11) | bond heart (8) | time per profile |
|---|---|---|---|---|
| whole frame, native | 37/37 | 7 | 0 | ~0.8 s |
| whole frame, 2x | 37/37 | 8 | 1 | ~1.2 s |
| two panels | 37/37 | 8 | 5 | ~1.0 s |

All numeric fields (including the `Lv.N` strip Tesseract could not read) are
perfect from one frame pass. Names and the heart are small overlaid text that
frame-level detection mostly misses, so the design is a **hybrid**: one
detection+recognition pass per frame for the panel numbers, plus recognition-only
reads of two small crops (name strip, bond heart; ~40 ms each, first benchmark:
names 10/11, heart 7/8) resolved by the dictionary.

Open items before the swap is final: held-out check on the 106 saved students
(`--reextract`, scored without labels against the dictionary and `expToLevel`),
model file hashes/sources/licences recorded before bundling, and the hosting
choice (in-process vs sidecar below).

Overlay implication (goal 3): Tesseract is spawned as a process per call
(~20 calls per profile, 100-300 ms each) which is fine for scanning but too
slow for a live overlay. An overlay needs an always-resident in-process model
and a read-only data path: the Store (JSON/HTTP/WebSocket) serves a thin
overlay window; screen reading and drawing stay separate processes.

## Bond rank up to 100

Bond is read from the pink heart by isolating dark glyphs inside the heart row
by row, so it does not depend on digit count; values are capped at 100 and
the 2-digit cases (2..13) are regression-tested. **No 3-digit sample exists
yet**: a web search found only text (max bond 100 for 5-star students, 20 for
3-star), no screenshots, and fan images are not safe to bundle. When the
account reaches a 3-digit bond, add that screenshot as a fixture; the
layout risk is the heart shrinking its font, which the guard below would show
as an implausible read.

## Risks to keep in mind

- Reading the screen is passive, but an overlay drawn over the game window and
  any automated clicking may conflict with the game's terms of service or
  anti-cheat. Keep the watcher screenshot-only, never touch game memory or
  files, and review the terms before shipping an overlay.
- Layout is calibrated for 4:3 windowed only (see README "Screen setup").

## Outcome: engine replaced, Rust port

The OCR engine decision above was acted on. The Go+Tesseract extractor was
ported to Rust (`rust/`) with PP-OCRv5 in process (oar-ocr for detection+
recognition, `ort` directly for the closed-set CTC reads).

- Names: 11/11 labelled screens and 106/106 saved frames, with no tuning tricks:
  ink-isolated line + all-dictionary CTC scoring. Syllables outside the model's
  vocabulary (e.g. 슌) are spelled with the nearest known syllable by likelihood.
- Bond (1..=100): 106/106 saved frames.
- Level: 105/105 against the exp table (the Tesseract path could not read it).
- ~0.9 s per profile frame including the closed-set reads, in process.
- Blank/off-layout frames are not "read": no ink means no name and no bond.
- Remaining gaps: equipment icons beyond tier 4, 3-digit bond sample, ability
  unlock (능력개방), 16:9/fullscreen, and live verification of the Rust scan and
  `--watch` against the game (the unit tests use a fake HCU runner).
