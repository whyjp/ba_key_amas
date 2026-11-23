# 0002 PP-OCR on ONNX Runtime in process, DirectML optional, closed-set reading for names

Status: Accepted

## Context
The first extractor (Go) used Tesseract as a process per call, 100 to 300 ms each and about 20 calls per
profile. That was fine for scanning and too slow for a live overlay. The question was the model, not the language.

## Decision
- Engine: PP-OCR models as ONNX (`PP-OCRv6_det_small` detection, `korean_PP-OCRv5_rec_mobile`
  recognition, its CTC dictionary), run in process by `oar-ocr` on top of `ort` (ONNX Runtime). No Python,
  no helper process.
- Student names are not read as free text. A second session of the same recogniser produces CTC
  probabilities. `decide` (`extract/src/cards.rs`) picks the name from the closed set of known names by likelihood,
  combined with an ink-width model of the letter count, and cross-checks it with a free read.
  Syllables the model lacks (for example 슌) are spelled with the nearest known syllable by likelihood.
- Acceleration: an optional DirectML build (`--features directml`, `DirectML.dll` beside the exe) runs the
  detector and recogniser on the GPU, with a probe that falls back to CPU. Recognition batch is fixed at 4.
  `BAOCR_OCR_DEVICE=cpu` forces the CPU. The portable package is built with the feature
  (`scripts/package-portable.ps1`).
- The closed-set session (`probs`, `choose`) still runs on the CPU.

## Alternatives (measured)
Measured on 11 labelled screens and 68 fields with `tools/ocr-bench`:

| Setup | Correct |
|---|---|
| Tesseract, heavily tuned on these screens | 66/68 |
| PP-OCRv5 Korean, detection plus recognition, untuned | 64/68 |
| PP-OCRv5 Korean, recognition only on tight crops | 53/68 |
| PP-OCRv5 Korean, recognition only on padded crops | 51/68 |

Frame-level detection reads all numeric fields (37/37) and misses small overlaid text (names, bond heart),
hence the closed-set reads for those. In the Rust port: names 106/106 and 11/11, bond 106/106, level 105/105,
about 0.9 s per profile frame.

Later tuning options (live fixtures, `docs/speed-research.md`) did not win. Changing the ORT thread count, graph
optimisation or memory pattern did not help, because the defaults are already optimal (one thread is 2.4 times
slower). Detector `limit_side_len` 640 is 25 percent faster but changes the text on 14 of 15 frames, so it was
rejected. A recognition batch other than 4 changes the text, and a single BitBlt capture changes the frame. Both
were rejected.

## Consequences
- DirectML: 15 fixture frames 4750 ms to about 1710 ms (2.7 times), quick formation 547 to 187 ms, text
  identical to CPU on 15 of 15 frames. First call about 0.44 s. Frame-rate impact on the game is unmeasured.
- Open speed options, in order: read only the grid band on the quick formation popup, skip the detector on
  fixed layouts, DirectML for the closed-set session.
- Models are not in git (`models/README.md` gives sources, hashes and licences).

## Evidence
`docs/speed-research.md`, `ocr/src/lib.rs`, `extract/src/cards.rs`, `tools/ocr-bench`.

## Revisit when
A newer PP-OCR release changes accuracy on the fixtures, or the game changes its font.
