# OCR models (not tracked by git)

Used by the Rust engine (`rust/crates/ocr`). Place these files here, or set `BAOCR_MODELS`.

| file | source | note |
|---|---|---|
| `PP-OCRv6_det_small.onnx` | RapidOCR 3.x model set (PaddleOCR PP-OCRv6 det small, ONNX) | detection |
| `korean_PP-OCRv5_rec_mobile.onnx` | RapidOCR 3.x model set (PaddleOCR korean PP-OCRv5 rec mobile, ONNX) | recognition |
| `korean_PP-OCRv5_dict.txt` | the `character` metadata inside the rec model, one entry per line | CTC dictionary |

SHA-1 prefixes when measured: det `05f8302fa4f1`, rec `ad263b1ba41a`, dict `0c75df948e5c`.
Check the PaddleOCR / RapidOCR licences (Apache-2.0) before redistributing the files.
