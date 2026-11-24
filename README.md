# Blue Archive 애드온

Blue Archive(글로벌 서버, 한국어 UI, 4:3 창 모드)용 Windows 애드온이다. 게임 화면을 읽고, 로컬 데이터베이스에서
임무나 레이드를 찾아, 게임 창 옆이나 위에 카드와 아이콘으로 보여준다. 버튼을 누르면 학생 명단을 JSON으로
스캔하기도 한다.

![아키텍처](docs/diagrams/crates.svg)

## 기능

| 기능 | 게임에 입력을 보냄 |
|---|---|
| 안내 카드: 화면 이름, 임무나 레이드 정보, 적 방어 타입과 공격 타입 비중, 편성 판정 | 아니오 |
| 빠른 편성 팝업의 아이콘: 학생과 슬롯별 적합 등급과 지형 적응 얼굴 아이콘 | 아니오 |
| 레이드 화면: 총력전, 모의총력전, 제약해제결전, 종합전술시험, 대결전 (레이드의 방어 타입과 공격 타입 비중을 담은 카드) | 아니오 |
| 전투 모드: 전투 중에는 카드가 상태만 보여 주고 화면 읽기를 멈춘다 | 아니오 |
| 학생 명단 스캔 (`students.json`, `index.json`, `details/NNN/student.json`) | 예, 스캔 버튼을 누른 뒤에만 |

카드는 항상 애드온의 상태를 보여준다. 애드온은 카드 머리글의 전원 아이콘이나 트레이 아이콘(오른쪽 클릭, 종료)으로
닫는다. 트레이 아이콘은 창이 멈춰도 동작한다.

## 실행

1. 포터블 빌드를 쓴다. `pwsh scripts/package-portable.ps1`을 실행하면 `D:\dist\ba-scanner`에 `ba-key-AMAS.exe`,
   `models/`, `wikidb/`, `DirectML.dll`이 생긴다. `ba-key-AMAS.exe`는 게임보다 먼저 켜도 되고 나중에 켜도 된다.
2. 게임을 4:3 창 모드로 설정하고 Windows HDR을 끈다. HDR이 켜져 있으면 판독에 쓰는 색이 바랜다.
3. 카드가 게임 옆에 나타난다. 모니터에 게임 옆 공간이 없으면 카드는 게임 안의 북마크 탭이 되고, 포인터를 올리면
   펼쳐진다.

설정은 `%APPDATA%\baocr\overlay.json`, 스캔 결과는 `%APPDATA%\baocr\students`, 입력 로그는
`%APPDATA%\baocr\input.log`에 저장한다. 치명적 오류는 `%APPDATA%\baocr\error.log`에 남는다.

## 카드 동작

- 배치는 앵커 네 곳 중 하나다. 바깥 왼쪽, 바깥 오른쪽, 안쪽 왼쪽 위, 안쪽 오른쪽 위다. 머리글이나 접힌 탭을 끌어
  옮기면 가장 가까운 앵커에 붙고, 그 위치를 기억한다. 자세한 내용은 [docs/card-dock.md](docs/card-dock.md)에 있다.
- 머리글의 고정 버튼은 카드를 펼친 채로 둔다. 고정하지 않으면 안쪽 카드는 탭으로 접힌다.
- 학생 목록 영역은 접을 수 있다. 접어도 목록을 스캔한 지 얼마나 지났는지는 보이고, 툴팁에 정확한 시각이 나온다.
- 스캔 중에는 스캔 자체의 입력을 뺀 마우스와 키보드 입력이 막힌다. ESC를 누르면 스캔이 멈추고 제어권이 돌아온다.
  카드에 이 내용이 표시된다.

## 명령

`cd rust && cargo build --release`로 한 번 빌드한 뒤 쓴다.

| 명령 | 동작 |
|---|---|
| `ba-key-AMAS` 또는 `ba-key-AMAS --assist` | 애드온 실행 |
| `ba-key-AMAS --doctor` | 게임 창, DPI, 모니터, 권한 상승, 포커스를 점검한다. 입력은 보내지 않는다 |
| `ba-key-AMAS --scan-arrows --from-first --out scan` | 명령줄에서 명단을 스캔한다. `--fast`를 붙이면 메뉴 단계를 건너뛴다 |
| `ba-key-AMAS --reextract scan [--ids 003,007]` | 저장한 프레임을 다시 읽는다. 게임 입력은 없다 |
| `ba-key-AMAS --upload https://<서버> [--public]` | 마지막 스캔을 웹서버(`web/`)에 올리고 키를 저장한다. 게임 입력은 없다 |
| `ba-key-AMAS --watch --out db` | 수동 관찰 업데이터 |
| `ba-key-AMAS --learn shots` | 플레이하는 동안 새 화면과 그 텍스트를 모두 저장한다. 입력은 보내지 않는다 |
| `ba-key-AMAS --image fixtures/detail-shun.png --out out` | 저장한 스크린샷 한 장을 파싱한다 |
| `cargo run -p baocr-wikidb --release` | SchaleDB에서 `wikidb/wiki.db`를 만든다 |
| `cargo run -p baocr-namedict` | 패치 뒤에 이름 사전을 갱신한다 |

환경 변수는 다음과 같다. `BAOCR_MODELS`(모델 폴더), `BAOCR_OCR_DEVICE=cpu`(GPU를 쓰지 않음),
`BAOCR_PROFILE=1`(단계별 시간), `BAOCR_INPUT_LOG=0`(입력 로그 끔),
`BAOCR_HCU_SETTLE_MS`(입력 뒤 대기 시간), `BAOCR_STATE_DUMP=<file>`(라이브 테스트를 위해 애드온의 상태를 기록),
`BAOCR_OVERLAY_CAPTURABLE=1`(스크린샷에 오버레이를 포함).

## 요구 사항

- Windows 10 또는 11, 4:3 창 모드의 게임, 게임 창 하나.
- `models/`의 OCR 모델([models/README.md](models/README.md) 참고)과 `wikidb/wiki.db`의 위키 데이터베이스(로컬에서
  빌드하며 재배포하지 않는다).
- DirectML을 지원하는 GPU는 OCR을 약 2.7배 빠르게 한다. CPU로도 동작한다.
- 빌드 출력: `CARGO_TARGET_DIR`을 여유 있는 드라이브로 지정한다. 저장소는 디버그 정보와 병렬 작업 수를 제한해서
  테스트 빌드가 메모리에 들어가게 한다([ADR 0009](docs/adr/0009-build-limits.md)).

## 문서

| 문서 | 내용 |
|---|---|
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md), [docs/architecture.html](docs/architecture.html) | 모듈, 실행 흐름, 정적 데이터와 동적 데이터, 스레드 |
| [docs/adr/](docs/adr/README.md) | 결정 기록 아홉 건 |
| [docs/hcu-native.md](docs/hcu-native.md) | 프로세스 내 데스크톱 제어, 입력 안전장치, `activate` 정책 |
| [docs/hcu-reference.md](docs/hcu-reference.md) | HCU 원본 참조, 이식 대응표, 아직 이식하지 않은 기능을 읽을 곳 |
| [docs/raid-data.md](docs/raid-data.md) | 레이드 표, 시즌, 조회, 레이드 화면 |
| [docs/enemy-weighting.md](docs/enemy-weighting.md) | 방어 타입과 공격 타입 비중을 HP로 가중하는 방법과 출처 |
| [docs/card-dock.md](docs/card-dock.md) | 앵커, 북마크 탭, 호버 시간, 스냅 |
| [docs/speed-research.md](docs/speed-research.md) | 지연이 생기는 곳, 측정한 선택지, DirectML |
| [docs/live-e2e.md](docs/live-e2e.md) | 실제 게임을 대상으로 하는 라이브 테스트 |
| [scanner/PORTING.md](scanner/PORTING.md) | Go 포트에서 가져온 모듈 계약 |
| [docs/history/](docs/history) | Go와 Tesseract 시기의 노트 |

## 현황

테스트와 라이브 실행으로 검증한 것: 레이드 화면의 화면 종류 인식, 데이터베이스와 대조한 보스 카드, 아이콘이 남던 문제의 수정, 접기, 고정, 호버, 입력 안전장치.
라이브로 아직 검증하지 못한 것: 프로세스 내 입력 경로로 하는 학생 106명 스캔, CPU와 DirectML의 비교, 클릭에서
카드까지 1초 미만의 지연(마지막 측정은 중앙값 1.7초였고, 결함 있는 터치스크린이 입력을 보내는 중에 쟀다), 실제
ESC와 입력 잠금, 트레이 메뉴, DirectML이 게임 프레임 속도에 주는 영향.

알려진 빈틈: 레이드의 부대는 최대 네 개인데 판정은 편집 중인 부대만 다룬다. 스크롤해야 보이는 Insane 이상의
행, 실제 대결전 화면, 종합전술시험 단계는 아직 학습하지 않았다. 세 자리 인연 랭크은 샘플이 없다. 4티어를 넘는
장비 아이콘은 검증하지 못했다. 4:3 창 모드가 아닌 레이아웃은 지원하지 않는다.

## 이력

처음 구현은 Go와 Tesseract였다. Rust 포트는 저장한 프레임 106장 전부와 라이브 전체 스캔에서 Go 구현과 같은
결과를 내고 나서 그것을 대체했다. 헬퍼 프로세스 방식의 데스크톱 제어는 프로세스 내 Win32 호출로 바뀌었다
([ADR 0003](docs/adr/0003-native-desktop-control.md)).

---

게임 <블루 아카이브>의 에셋 및 콘텐츠의 권리는 넥슨, 넥슨게임즈 및 Yostar에 있습니다.
