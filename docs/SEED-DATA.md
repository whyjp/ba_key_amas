# 시드 데이터 정책

수집한 데이터와 이미지는 연구 단계에서 확정해 저장소에 커밋한다. 빌드와 CI는 SchaleDB, 위키, ModelScope 같은 외부 저장소에서 아무것도 내려받지 않는다.

| 데이터 | 위치 | 갱신 방법 |
|---|---|---|
| OCR 모델 3개 | `models/`, 무결성은 `models/SHA256SUMS` | 파일을 바꾸고 `sha256sum *.onnx *.txt > SHA256SUMS` |
| 위키 DB 시드 | `wikidb/wiki.db`, 스키마는 `wikidb/schema.sql` | `cd scanner && cargo run -p baocr-wikidb --release`를 연구 단계에서 실행하고 결과를 커밋 |
| 웹 이미지 | `web/public/` (카드, 입상 `standing/`, 로비 `lobby/` 등) | `cd web && npm run assets`를 연구 단계에서 실행하고 결과를 커밋 |
| 웹 D1 스키마 | `web/migrations/` | 새 번호의 마이그레이션 추가 |

- `web/scripts/.cache/`(원본 내려받기 캐시, 약 450 MB)만 커밋하지 않는다. 결과물이 이미 `web/public/`에 있어 빌드에 필요하지 않다.
- 릴리스 워크플로는 `models/SHA256SUMS`와 `scripts/check_wikidb.py`로 커밋된 파일만 검사한다.
- 출처와 내려 달라는 요청에 대한 절차는 [ASSETS.md](ASSETS.md)에 있다.
