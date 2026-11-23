# HCU 원본 참조와 추가 이식 안내

`crates/hcu`의 데스크톱 제어는 Microsoft의 Hybrid Computer Use(HCU)를 명세로 삼아 Win32로 직접 구현한 것이다.
이 문서는 어느 원본의 어느 부분을 따랐는지 기록하고, 아직 따오지 않은 기능을 나중에 이식할 때 어디를 읽어야 하는지 알려 준다.
구현 쪽 설명은 [hcu-native.md](hcu-native.md), 결정 배경은 [결정 기록 0003](adr/0003-native-desktop-control.md)에 있다.

## 원본

| 항목 | 내용 |
|---|---|
| 저장소 | https://github.com/microsoft/Hybrid-Computer-Use |
| 라이선스 | MIT (Copyright 2026 Hybrid Computer Use authors). 전문은 `third_party/hcu-reference/LICENSE`와 `THIRD_PARTY_NOTICES.md`에 있다. |
| 참조한 커밋 | `8ce82b310b014ab1133b28febc2eca81527e43e6` (2026-11-18), 원본 `VERSION` 1.0.1 |
| 구성 | Go로 쓴 코어(`computer-use/internal`), C#으로 쓴 Windows provider(`computer-use/native/windows`), macOS용 Swift, Linux용 Python |
| 이 프로젝트가 쓰는 부분 | Windows provider 중 화면 캡처, 좌표 변환, 입력 주입, 창 목록, 활성화와 Go 코어의 GUI 요청 규칙. AX(접근성)와 daemon, MCP는 쓰지 않는다. |

원본 소스는 명세로만 읽었고 컴파일하지 않는다. 구현 코드는 원본을 복사하지 않고 새로 썼다.
나중에 이식할 때 원본을 바로 열어 볼 수 있도록 위 커밋의 관련 파일을 `third_party/hcu-reference/`에 그대로 보관했다.
이 폴더는 빌드 대상이 아니고 파일 내용은 수정하지 않는다.

## 보관한 원본 파일

| 보관 위치 | 원본 경로 |
|---|---|
| `windows/NativeDesktop.cs` | `computer-use/native/windows/ComputerUse.Windows.Runtime/NativeDesktop.cs` |
| `windows/NativeDesktop.Capture.cs`, `.StableCapture.cs`, `.Input.cs`, `.Zoom.cs`, `.TextInput.cs` | 같은 폴더의 같은 이름 |
| `windows/WindowsDesktopImageGeometry.cs`, `WindowsDesktopZoom.cs` | 같은 폴더 |
| `windows/WindowsKeyboardFocus.cs`, `WindowsKeyMapping.cs` | 같은 폴더 |
| `windows/WindowsAppInventory.cs`, `WindowsWindowIdentityPolicy.cs`, `WindowsDesktopLayoutDiagnostics.cs` | 같은 폴더 |
| `go/gui_service.go`, `gui_zoom.go`, `request_replay.go` | `computer-use/internal/core/` |
| `docs/CODE.md`, `docs/PLATFORMS.md` | `docs/` |

보관하지 않은 원본 파일은 같은 커밋으로 가져온다.

```
gh api "repos/microsoft/Hybrid-Computer-Use/contents/<원본 경로>?ref=8ce82b310b014ab1133b28febc2eca81527e43e6" --jq .content | base64 -d
```

원본이 앞서 나가 있으면 이 커밋과 비교해 바뀐 파일만 읽는다. 기준 커밋을 올릴 때는 이 문서의 표와 `hcu-native.md`의 커밋 표기를 함께 고친다.

## 이식한 부분과 원본 위치

| 기능 | 원본 | 우리 코드 | 다른 점 |
|---|---|---|---|
| 전달 해상도(최대 1366x768, 확대 없음) | `WindowsDesktopImageGeometry.DeliveredSize` | `native.rs` `delivered_size` | 같다 |
| 프레임 좌표를 화면 픽셀로, 화면 픽셀을 `SendInput` 절대 좌표로 | `ToNativeCoordinate`, `ToSendInputAbsolute` | `to_native`, `to_absolute` | 같다 |
| 전체 가상 데스크톱 캡처 | `NativeDesktop.Capture.cs` | `WinDesktop::grab`(BitBlt) | 원본은 `CopyFromScreen`, 결과는 같은 화면이다 |
| 줌 영역 계산 | `NativeDesktop.Zoom.cs`, `WindowsDesktopZoom.cs` | `zoom_geometry`, `NativeRunner::zoom` | 같다 |
| 클릭과 휠 | `NativeDesktop.Input.cs`의 `MovePointer`, `Click`, `Scroll` | `WinDesktop::click`, `wheel` | 왼쪽 버튼 클릭과 세로 휠만 구현했다 |
| 오래된 프레임 판정 | `WindowsDesktopZoom.RegionStillValid`, 레이아웃 스냅샷 | `guard.rs`와 `NativeRunner` | 프레임마다 레이아웃을 들고 있고, 우리 오버레이 창은 제외한다 |
| 입력 후 화면 안정 대기 | `NativeDesktop.StableCapture.cs` | `native.rs`의 `Settle` | 대상 창 영역만 두 번 비교하고 상한(기본 600 ms)이 있다 |
| 창 목록과 활성화 | `NativeDesktop.cs`의 `ListWindows`와 활성화, `WindowsAppInventory.cs` | `WinDesktop::windows`, `activate` | 활성화는 일부러 다르게 했다(아래) |

활성화는 원본처럼 포커스를 억지로 가져오지 않는다. 키 입력이나 입력 큐 연결을 쓰지 않고 `SetForegroundWindow`만 부르며,
거부되면 사용자에게 게임 창을 한 번 클릭해 달라고 알린다. 이유는 [hcu-native.md](hcu-native.md)의 "게임 창 활성화"에 있다.

## 아직 이식하지 않은 부분과 읽을 곳

필요가 생기면 아래 순서로 읽는다. 괄호 안은 보관한 원본 파일이다.

| 기능 | 읽을 곳 | 이식 때 주의 |
|---|---|---|
| 드래그 | `NativeDesktop.Input.cs`의 `Drag`(경로 검증, 누름 80 ms 대기, 이동 간격 12 ms, 실패 시 버튼 해제) | 누른 버튼이 남지 않도록 해제를 보장하는 보호 객체를 쓴다. 하네스의 `Held` 패턴이 예다. |
| 오른쪽, 가운데, 더블 클릭, 뒤로, 앞으로 버튼 | `NativeDesktop.Input.cs`의 `Click`(`button`, `count`) | 새 입력 종류마다 감사 로그와 안전장치 시험을 추가한다. |
| 키 입력과 조합키 | `NativeDesktop.Input.cs`의 `PressKeys`, `WithHeldKeys`, `WindowsKeyMapping.cs` | 키 누름이 남는 문제가 가장 위험하다. 실패 시 모든 키를 해제하는 경로를 먼저 시험한다. |
| 텍스트 입력 | `NativeDesktop.TextInput.cs`, `WindowsKeyboardFocus.cs` | 이 게임에는 필요하지 않다. 입력창이 있는 다른 용도가 생길 때만 본다. |
| 같은 이름 창 구분 | `WindowsWindowIdentityPolicy.cs` | 지금은 정확한 제목 일치만 쓴다. 제목이 겹치면 여기를 읽는다. |
| 요청 재실행 방지와 daemon | `go/request_replay.go`, `docs/CODE.md` | 프로세스를 분리하지 않는 한 필요 없다. |
| AX(접근성 트리) | `computer-use/native/windows`의 `Uia*.cs`(보관하지 않음) | 게임 화면에는 접근성 트리가 없다. |
| GUI 요청 조립 | `go/gui_service.go`, `go/gui_zoom.go` | 요청 검증 규칙을 확인할 때 참고한다. |

## 이식 절차

1. 보관한 원본 파일에서 해당 부분을 읽고, 입력과 출력의 규칙(좌표계, 대기 시간, 실패 시 정리)을 `native.rs` 안 주석이 아니라 이 문서의 표에 먼저 적는다.
2. `Desktop` 트레이트에 필요한 메서드를 추가하고, 가짜 데스크톱으로 정상 경로와 거부 경로를 시험한다.
3. 입력을 보내는 기능이면 감사 로그 항목, 부분 실패(`effect_unknown`), 눌림 해제를 구현하고 시험한다.
4. 읽기 전용 핸들과 `Exclusive` 핸들 중 어느 쪽에 둘지 정한다. 입력은 항상 `Exclusive`다.
5. 실제 게임에서 확인할 항목은 라이브 테스트에 시나리오로 추가한다(사용자가 PC를 비웠을 때만 실행한다).
6. 이 문서의 표와 [hcu-native.md](hcu-native.md)의 "이식하지 않은 부분"을 함께 고친다.

## 도우미 프로그램과 `vendor/`

이전 방식인 `computer-use.exe`(Go와 .NET)와 `BAOCR_HCU=process` 폴백은 지웠다. 106명 스캔을 같은 게임 상태에서 두 번 돌려 읽은 값이 모두 같음을 확인했고(2026-11-24), 원본 파일의 보관 사본은 `third_party/hcu-reference/`에 있다.
`vendor/ocr`(Tesseract 런타임)와 `vendor/micromamba`는 Go 시대 도구라 코드가 참조하지 않아 이미 지웠다.
