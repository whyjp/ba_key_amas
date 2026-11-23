# 0009 RAM과 디스크 사용을 제한하는 빌드 설정

상태: 확정

## 배경
여러 워크트리에서 `cargo test --workspace`를 병렬로 돌리면 테스트 바이너리 열두 개 남짓이 링크되고 각각 약 2 GB를 쓴다.
PC의 RAM이 바닥나서 게임과 터미널이 죽었고 D: 드라이브도 가득 찼다(target 폴더가 30 GB 이상).

## 결정
- 저장소에 `[profile.dev] debug = "line-tables-only"`와 `[build] jobs = 4`를 둔다.
- 워크트리는 D:에만 만들고 워크트리마다 target 디렉터리를 따로 쓴다. 워크트리 하나에서 cargo 명령은 한 번에 하나만
  돌린다. 브랜치를 병합하면 `debug/incremental`과 target 디렉터리를 지운다.
- 에이전트는 `CARGO_BUILD_JOBS=3`과 디버그 정보 끔으로 실행하고, 반복 작업 중에는 `-p <crate>`를 쓴다.

## 대안
- target 디렉터리를 공유하면 잠금 경합이 생기고 기능 조합이 섞인다.
- 워크트리 수를 줄이면 작업이 느려진다. 제한이 지켜지면 줄일 필요도 없다.

## 결과
- 백트레이스에는 줄 번호가 남지만 변수 검사는 할 수 없다.
- 포터블 빌드는 여전히 `--release`와 `--features directml`을 쓴다(`scripts/package-portable.ps1`).

## 근거
이 저장소 이력에 남은 크래시와 약 100 GB의 빌드 산출물 정리.

## 다시 볼 조건
기여자 대부분이 RAM과 디스크가 더 큰 기기를 쓰게 될 때.
