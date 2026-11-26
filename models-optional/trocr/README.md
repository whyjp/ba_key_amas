# TrOCR 이름판 모델 (저장소에만 보관, 배포하지 않음, 켜지 않음)

`team-lucid/trocr-small-korean`을 ONNX fp16w로 나눈 한 쌍이다. 쓰임새와 측정은 `docs/trocr-evaluation.md`에 있다.

| 파일 | 크기 | 역할 |
|---|---|---|
| `encoder.onnx` | 46 MB | 이미지 인코더 |
| `decoder.onnx` | 63 MB | 글자 디코더 |

- **배포하지 않는다.** `models/` 밖에 두었으므로 `scripts/package-portable.ps1`이 릴리스 zip에 넣지 않는다. 같은 스크립트가 zip에 TrOCR 파일이 섞이면 실패한다.
- **켜지 않는다.** 코드는 cargo feature `trocr`(기본 꺼짐) 뒤에 있고, 기본 빌드에는 들어가지 않는다.
- **검수할 때만 쓴다.** `cargo build --release -p baocr --bin ba-key-AMAS --features trocr` 후 `BAOCR_TROCR=models-optional/trocr ba-key-AMAS --eval-plates scanner/fixtures/plates`.
- 무결성은 `SHA256SUMS`. 재배포하기 전에 원 모델의 라이선스를 모델 카드에서 확인한다.
