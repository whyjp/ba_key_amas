# OCR 모델 (git이 추적하지 않음)

Rust 엔진(`scanner/crates/ocr`)이 쓰는 파일이다. 이 폴더에 두거나 `BAOCR_MODELS`를 설정한다.

| 파일 | 출처 | 비고 |
|---|---|---|
| `PP-OCRv6_det_small.onnx` | RapidOCR 3.x 모델 세트 (PaddleOCR PP-OCRv6 det small, ONNX) | 검출 |
| `korean_PP-OCRv5_rec_mobile.onnx` | RapidOCR 3.x 모델 세트 (PaddleOCR korean PP-OCRv5 rec mobile, ONNX) | 인식 |
| `korean_PP-OCRv5_dict.txt` | rec 모델 안의 `character` 메타데이터, 한 줄에 항목 하나 | CTC 사전 |

측정 당시 SHA-1 앞부분: det `05f8302fa4f1`, rec `ad263b1ba41a`, dict `0c75df948e5c`.
파일을 재배포하기 전에 PaddleOCR / RapidOCR 라이선스(Apache-2.0)를 확인한다.
