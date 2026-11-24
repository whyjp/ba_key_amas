# Plate OCR measurement

The duty-selection list shows each student's name on a small plate. The letters are 12 to 14 px tall in the native 1280x960 frame (frames are not upscaled), set in a soft face. The profile screen uses a large font and reads 106 of 106 exactly, so this page is only about the plate path.

The goal is a more accurate raw OCR reading, not a better dictionary lookup. Nothing here applies the name dictionary: every score below compares the engine's raw text to the label.

## Harness

`ba-key-AMAS --eval-plates <dir>` runs the real Rust engine (`baocr-ocr`, the same models and ONNX Runtime as the scan) over every PNG listed in `<dir>/labels.json`. `scripts/eval-plates.ps1` wraps it with the default fixtures and model folder. It needs no game and sends no input.

It prints two scores, each an exact match of the text with spaces removed, the list of wrong readings, and the mean milliseconds per plate (CPU, one process, after one warm-up read).

| Mode | What it runs |
| --- | --- |
| detector alone | `Ocr::boxes` on the plate picture, boxes joined left to right |
| scan plate path | the same, then `reread_plate` of `progress_scan.rs`: the plate is read again at 2x, 3x and 4x by the full pipeline and a reading that two of the four agree on wins |

`read_plate` in `progress_scan.rs` is the per-plate step of `reread_plates`, so the harness scores the code the scan runs, not a copy.

## Fixtures

`scanner/fixtures/plates/` holds 160 PNG crops of name plates (about 700 KB) and `labels.json`. They were cut from 14 duty-list frames of the saved run `20261124` (`frame-007` to `frame-020`) at plate boxes found by plate colour. Each entry keeps its source frame and box, the label, and `old_engine`, the text the previous engine read at that box.

Caveat about the labels. They were assigned automatically: each reading of the previous engine was matched to the 106 known student names and accepted, and only 6 entries where that disagreed with what is on the plate were fixed by hand. The benchmark therefore leans toward the previous engine: a plate it misreads in a way that still looks like another real name could carry a wrong label, and a plate it reads right is more likely to be labelled right. A score on this set is best used to compare two code versions on the same set, not as an absolute accuracy figure. It is also 160 plates from one account's frames, so one plate is 0.6 points and small differences are noise-sized.

The crops are plates alone, without the lobby around them as in a frame, so the detector sees a different picture than in a scan. That is why "detector alone" here (152) is not the 154 measured on frame boxes.

## Baseline (U0, unchanged code, commit of this page)

Measured with the Rust engine on CPU, twice with identical accuracy.

| Mode | Exact | ms per plate |
| --- | --- | --- |
| detector alone | 152/160 | 14 to 16 |
| scan plate path | 155/160 | 74 to 81 |

Wrong readings of the scan plate path (label, read):

- 슌 read 순 (2 plates)
- 아야네 read 아네
- 이즈미(새해) read 이즈미(새해
- 츠루기(수영복) read 초루기(수영복)

Wrong readings of the detector alone: 시로코*테러 read 시로코테러 (2), 슌 read 순 (2), 유우카(파자마) read 유우캐(파자마), 히나 read 하나, 아야네 read 아네, 아야네(수영복) read 아이네(수영복).

The researcher's Python measurement with the PP-OCRv5 mobile recogniser on tight crops gave 152/160 on raw colour and 157/160 as 3-channel grayscale. That is a different input (tight crop, recogniser only) from the scan path measured here, and it was measured outside the Rust engine, so it is a lead and not a result.
