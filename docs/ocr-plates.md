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

## Experiment: 3-channel grayscale (U1)

Grayscale (BT.601 luma copied into all three channels) was applied at four places and each was scored with the harness on the same 160 plates, against the baseline above. Experiment builds switched the variants by an environment variable that is not kept in the code.

| Variant | Detector alone | Scan plate path |
| --- | --- | --- |
| baseline, colour everywhere | 152 | 155 |
| grayscale in the enlarged re-reads | 152 | 153 |
| grayscale on the plate before detection (whole-frame analog), colour re-reads | 155 | 154 |
| colour re-reads plus the same three gray re-reads as extra votes | 152 | 155 (169 ms, twice the time) |

Verdict: not verified. No variant raised the scan plate path above 155, so the code keeps colour and is unchanged.

What each variant did:

- Gray re-reads lost 시로코*테러 (2 plates, read 시로코테러) and 준코(새해), and fixed 츠루기(수영복). Net 153.
- Gray detection fixed 유우카(파자마), 히나 and 아야네(수영복) in the detector alone (152 to 155), but it reads the asterisk of 시로코*테러 as 츠 (시로코츠테러, 2 plates), which is the error the scan already had on that plate. The scan path then starts from that reading, so it ends at 154.
- Gray as extra votes swaps 츠루기(수영복) for 준코(새해), at double the time.

Because grayscale was not kept, the large-font path (profile frames, 106 names) was not changed and its re-extraction check was not run.

Wrong in every variant: 슌 read 순 (2 plates), 아야네 read 아네 and 이즈미(새해) read 이즈미(새해 (a dropped closing bracket). A colour or luma change does not move the first three. The researcher's 157/160 from Python was on tight crops with the recogniser alone, so the Rust scan path (detect, then re-read through the full pipeline) is not reproduced by a tight-crop gray recognition. A harness mode that reads a tight crop with the recogniser only (`Ocr::line`) would be the next measurement.

## A dropped syllable: 아야네 read as 아네

The plate path now settles it from the picture's width, not from another model. The letters' box on a plate is 16 px for one syllable, 34 to 38 for two, 43 to 50 for three and 59 for four (measured on the 160 labelled plates, frame scale), so the box tells how many syllables the plate holds. When a reading of a plain name is not a dictionary name and has fewer syllables than that, the names of the right length that are closest to the reading (`baocr_names::nearest_of_length`, within two jamo) are the candidates. A single one is taken. Several, as with 아야네 and 아카네 for "아네", are scored by the recogniser against the enlarged plate, and the best fit wins. Plates with an outfit in brackets or a `*` are left as read. On the labelled plates the scan path goes from 155/160 to 156/160 exact, with no plate changed for the worse. The four left are 순 for 슌 (2, set right by the dictionary later), 이즈미(새해 and 초루기(수영복).
