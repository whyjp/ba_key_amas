# 배포 런북: GitHub 빌드, GitHub 릴리즈, Cloudflare 배포

흐름은 한 줄이다. 태그를 밀면 GitHub가 애드온을 빌드해 릴리즈로 올리고, 웹 서버는 GitHub에서 Cloudflare Workers Builds로 배포되며 그 다운로드 링크(`ADDON_URL`)는 이 저장소의 `releases/latest`를 가리킨다.

```
태그 v0.2.0 푸시
  -> release.yml: 모델 내려받기(sha256 검증) -> wiki.db 빌드 -> 빌드 -> 테스트 -> 포터블 zip -> SHA256SUMS -> 실행 스모크 테스트 -> GitHub Release
main 푸시 (web/ 변경)
  -> web-ci.yml: npm test, tsc, wrangler 번들 드라이런, 문서 검사, 릴리즈 계약 검사, 이미지 크기 예산
  -> Cloudflare Workers Builds (Root directory web): npm ci -> d1 migrations apply -> wrangler deploy
```

## (a) 준비해 둔 것

| 파일 | 역할 |
|---|---|
| `.github/workflows/release.yml` | 태그 `v*`(또는 수동 실행)에서 애드온을 빌드해 Release를 만든다. 쓰기 권한은 `publish` 작업에만 있다 |
| `.github/workflows/web-ci.yml` | `web/`와 `scripts/` 변경에서 테스트, 타입 검사, `wrangler deploy --dry-run`, 문서 검사, 릴리즈 계약 검사를 한다 |
| `scripts/release-check.ps1`, `release-contract.json` | 릴리즈와 웹 서버 사이의 계약, 에셋 존재, 이미지 크기 예산 검사 |
| `scripts/check_wikidb.py` | 방금 만든 `wiki.db`의 스키마 버전과 표 내용을 검사한다 |
| `scripts/package-portable.ps1` | 포터블 폴더와 zip, `SHA256SUMS`를 만든다. 로컬과 CI가 같은 스크립트를 쓴다 |
| `web/wrangler.jsonc` | D1 id가 들어 있고, `KEY_PEPPER`와 `ADDON_SIGNING_KEY`가 없으면 배포가 실패하게 `secrets.required`를 넣었다 |
| `docs/ASSETS.md` | 이미지 정책, 다시 만들기, 권리자 삭제 요청 절차 |

### 에셋: 저장소에 커밋하고 Cloudflare 빌드는 `npm ci`만

웹 이미지와 카탈로그(약 59 MB)는 저장소에 들어 있다. Workers Builds의 빌드 제한 시간은 무료와 유료 모두 20분, CPU 2 vCPU(유료 4), 메모리 8 GB, 디스크 20 GB다(공식 문서 Limits & pricing). 이 PC에서 잰 `npm run assets`는 아래와 같다.

| 단계 | 시간 | 결과 |
|---|---|---|
| `pip install` (opencv, pillow, numpy) | 18초 | |
| `extract-assets.py` (SchaleDB) | 589초 | public 50.2 MB, 1164개 |
| `extract-halos.py` (위키) | 50초 | public 61.4 MB |
| `extract-lobby.py` (위키 원본 1.5 ~ 2.5천 픽셀) | 317초 | 원본 캐시 476 MB |
| `crop-art.py` | 약 209초 | |
| `audit-assets.py` | 16초 | public 101.2 MB, 1977개 |
| 합계 | 1199초 (약 20.0분) | 내려받은 캐시 476 MB, 결과 101 MB |

합계가 제한 20분과 같아서 `npm ci`(34초)와 배포까지 더하면 넘는다. 게다가 세 외부 사이트가 살아 있고 그대로여야 하고 Python과 OpenCV를 빌드 이미지에서 쓸 수 있어야 한다(확인할 수 없다). 그래서 결과물을 커밋했다. 정책과 다시 만드는 법은 [ASSETS.md](ASSETS.md)에 있다. Python과 내려받기 시간은 이제 Cloudflare 빌드에 없다.

wrangler가 올릴 정적 에셋은 `wrangler deploy --dry-run`에서 1445개로 읽혔고(`public/`의 파일은 1437개, 59,787,476 바이트, 가장 큰 파일 232,935 바이트), 공식 한도(Workers Free 20,000개, Paid 100,000개, 파일당 25 MiB)보다 훨씬 작다.

## (b) 사용자만 할 수 있는 일

순서대로 한다.

1. **GitHub Actions 켜기.** 저장소 Settings, Actions, General에서 Actions를 허용하고 Workflow permissions는 기본(Read repository contents)으로 둔다. 쓰기는 `release.yml`의 `publish` 작업이 자기 `permissions`로 올린다.
2. **D1 데이터베이스.** 이미 콘솔에서 `ba-key-amas`를 만들었고 그 id가 `web/wrangler.jsonc`의 `database_id`에 들어 있다(식별자이지 비밀이 아니다). 할 일은 없다.
3. **비밀값 두 개.** Cloudflare 대시보드 Workers & Pages, 해당 Worker, Settings, Variables and Secrets에 Secret으로 `KEY_PEPPER`를 넣는다(또는 `npx wrangler secret put KEY_PEPPER`). 길고 무작위인 값 하나를 정해 계속 쓴다. 바꾸면 기존 계정이 모두 끊긴다. 둘째로 `ADDON_SIGNING_KEY`(아래 서명 키 절)가 Worker와 GitHub Actions에 같은 값으로 있어야 한다. 이미 넣어 두었다고 들었지만 이 저장소에서 확인하지는 못했다. 둘 중 하나라도 Worker에 없으면 `secrets.required` 때문에 배포가 실패한다.
4. **Cloudflare에 저장소 연결.** Workers & Pages, Create application, GitHub에서 `whyjp/ba_key_amas`를 고르고 아래처럼 설정한다. Worker 이름은 `wrangler.jsonc`의 `name`(`ba-key-amas`)과 같아야 한다. 다르면 빌드가 실패한다.
   - Root directory: `web`
   - Build command: `npm ci`
   - Deploy command: `npx wrangler d1 migrations apply ba-key-amas --remote && npx wrangler deploy`
   - 선택: Build watch paths에 `web/*`를 넣으면 애드온만 바뀐 푸시로는 웹이 배포되지 않는다.
5. **첫 애드온 태그.** `scanner/Cargo.toml`의 `[workspace.package] version`과 같은 값으로 `git tag v0.1.0 && git push origin v0.1.0`을 한다. 태그와 Cargo 버전이 다르면 `release-check`가 첫 단계에서 실패한다. 버전을 올릴 때는 `Cargo.lock`도 함께 갱신해 커밋한다(`cargo build`가 갱신하고, CI는 `--locked`로 빌드한다).

미리 알아 둘 것이 하나 있다. Workers Builds의 빌드 토큰이 `d1 migrations apply --remote`를 할 권한이 있는지는 공식 문서에 적혀 있지 않다. 첫 빌드 로그에서 권한 오류가 나면 Deploy command를 `npx wrangler deploy`로 줄이고, 마이그레이션은 PC에서 `npm run db:remote`로 배포 전에 적용한다.

## 서명 키 `ADDON_SIGNING_KEY`

애드온은 업로드, 공개 여부, 삭제 요청에 HMAC-SHA256 서명(`x-amas-sig`)을 붙이고, 서버는 같은 키로 검증한다. 키는 소스에 없다. 공식 릴리즈 빌드만 컴파일 때 `BAOCR_ADDON_SIGNING_KEY`로 받아 실행 파일에 넣는다(`scanner/crates/cli/src/upload.rs`의 `signing_key()`). 소스에서 직접 빌드한 애드온은 키가 없어 서버가 거부한다.

- `release.yml`은 이 비밀값을 두 단계에만 준다. 길이 검사 단계(16자 미만이거나 비어 있으면 실패, 값은 출력하지 않는다)와 포터블 패키지 단계(여기서만 컴파일에 넣는다). 둘 다 제3자 액션이 없는 pwsh 단계다. 작업 전체나 워크플로 전체의 `env`에는 두지 않았고, 테스트와 스모크 테스트 단계에는 없다.
- 트리거는 태그 푸시와 수동 실행뿐이다. `pull_request` 트리거가 없으므로 포크의 PR은 이 워크플로를 돌릴 수 없고 비밀값도 받지 못한다. 두 작업 모두 `whyjp/ba_key_amas`가 아니면 건너뛰고, 수동 실행은 `main`이나 `v*` 태그에서만 돈다.
- 스모크 테스트는 `ba-key-AMAS --build-info`가 `signed build: yes`를 출력하는지 본다. 이 값은 컴파일 때 키가 있었는지(16자 이상)만 알려 주고 키 자체는 출력하지 않는다.
- 빌드 캐시에는 `scanner/target`을 넣지 않았다. 키가 들어간 빌드 산출물이 캐시에 남지 않게 하기 위해서다. 대신 릴리즈 빌드가 느리다.
- 실행 파일에 들어간 키는 `strings` 같은 도구로 꺼낼 수 있다. 서명은 소스 빌드와 웹 브라우저의 위조 요청을 막는 장치이지, 공개 배포된 실행 파일을 가진 사람에게 숨겨진 비밀이 아니다.

**키를 바꿀 때.** 서버가 두 키를 모두 받는 상태를 먼저 만들고 그다음에 릴리즈한다.
1. 새 키를 만든다. (`openssl rand -hex 32`)
2. Worker 비밀값을 `ADDON_SIGNING_KEY`는 새 키로, `ADDON_SIGNING_KEY_PREV`는 옛 키로 둔다. 이제 옛 애드온과 새 애드온을 모두 받는다.
3. GitHub Actions의 `ADDON_SIGNING_KEY`를 새 키로 바꾸고 새 버전 태그로 릴리즈를 낸다. 서버를 먼저 새 키만 받게 바꾸면 이미 배포된 애드온이 모두 거부되므로 순서를 지킨다.
4. 옛 애드온이 업데이트될 시간을 준 뒤 `ADDON_SIGNING_KEY_PREV`를 지운다.

## (c) 릴리즈 순서: 애드온 먼저, 서버는 나중에

1. 애드온 변경을 `main`에 합친다.
2. `scanner/Cargo.toml`의 버전을 올리고 `v<버전>` 태그를 민다. `release.yml`이 끝나 Release에 zip과 `SHA256SUMS`가 보일 때까지 기다린다.
3. 그 Release가 `releases/latest`로 열리는지 눈으로 확인한다.
4. 그다음에 서버의 최소 요구를 올린다. 해당하는 것만 고친다. `web/src/version.ts`의 `MIN_PROTOCOL`, `MIN_ROSTER_SCHEMA`, `MIN_PROGRESS_SCHEMA`, 그리고 `release-contract.json`의 `min_addon_version`. 이 변경을 `main`에 합치면 Cloudflare가 배포한다.

`release-contract.json`의 `min_addon_version`은 서버가 요구하는 가장 낮은 애드온 버전이다. 서버 코드에는 애드온 버전의 최소값이 따로 없고 업로드 규약 번호만 있어서, 이 값을 계약 파일에 두었다. `scripts/release-check.ps1`은 다음에서 실패한다. 태그가 이보다 낮거나 `scanner/Cargo.toml`과 다를 때, 업로드 규약 번호가 서버 범위 밖일 때, `ADDON_URL`이 이 저장소의 releases 주소와 다를 때, 마이그레이션 번호에 빈 곳이 있을 때, 학생 한 명이라도 카드 이미지가 없을 때, 이미지를 5 MB 넘게 바꿀 때(`large-assets` 라벨이 없으면). `web-ci`와 `release.yml`이 모두 돌린다.

순서가 뒤집히면, 즉 서버를 먼저 올리면 이전 애드온은 모두 426을 받는다. 응답은 `releases/latest` 링크를 안내하지만 그 링크에는 아직 새 애드온이 없다. 사용자는 업데이트할 방법이 없는 채로 업로드가 막힌다. 되돌리는 방법은 (d)의 롤백이다. 또 서버가 새 형식을 받으려면 마이그레이션이 먼저 필요하므로, 마이그레이션은 이전 코드도 견디는 추가형(새 열이나 표)으로 쓴다. 롤백은 코드만 되돌리고 DB는 되돌리지 않기 때문이다.

`releases/latest`는 공식 문서(REST API, Get the latest release)대로 초안과 사전 릴리즈를 뺀 가장 최근 릴리즈를 가리킨다. 이 저장소에 애드온이 아닌 릴리즈를 만들 일이 있으면 사전 릴리즈로 표시하거나 `--latest=false`를 준다.

## (d) 롤백

- **웹 코드.** 대시보드 Workers & Pages, 해당 Worker, Deployments에서 이전 버전 옆 메뉴의 Rollback을 고르거나 `npx wrangler rollback`을 실행한다. 최근 100개 버전까지, 30일이 지나면 지워질 수 있다. 연결된 리소스(D1)는 되돌려지지 않는다.
- **D1.** 마이그레이션은 앞으로만 간다. 데이터를 되돌려야 하면 `npx wrangler d1 time-travel restore ba-key-amas --timestamp=<유닉스 초>`를 쓴다. 제자리를 덮어쓰는 위험한 작업이고, 출력되는 이전 북마크로 다시 되돌릴 수 있다. 보존 기간은 무료 7일, 유료 30일이다.
- **애드온 릴리즈.** 잘못 올린 Release는 GitHub에서 삭제하거나 Pre-release로 바꾸면 `releases/latest`가 이전 Release로 돌아간다. 이미 받은 사용자의 파일은 남으므로 고친 새 버전을 올리는 쪽이 낫다.
- **이미지.** 이미지는 코드와 같은 커밋에 있어서 웹 코드를 롤백하면 함께 돌아간다. 이미지를 역사에서 지워야 하면 [ASSETS.md](ASSETS.md)의 삭제 요청 절차를 따른다.

## (e) 검증하지 못한 것

| 항목 | 상태 |
|---|---|
| 워크플로 YAML을 GitHub에서 실제로 실행 | 못 했다. 구문만 읽어 봤고 액션의 커밋 sha는 `git ls-remote`로 확인했다. 러너(`windows-latest`)의 Rust와 pwsh, 캐시, `gh release create`, 권한은 첫 태그에서 확인된다 |
| 서명 키 비밀값 | 저장소에 값이 있는지 못 본다(보면 안 된다). 길이 검사 단계가 첫 실행에서 알려 준다. 가짜 키(20자)로 로컬 빌드해 `--build-info`가 `signed build: yes`, 키 없이 빌드하면 `no`를 내는 것과, 가짜 키가 `yes` 빌드의 실행 파일 바이트에는 있고 `no` 빌드에는 없는 것을 확인했다 |
| 모델 내려받기 | URL과 sha256 세 개는 ModelScope의 RapidOCR v3.10.0에서 받아 로컬 모델과 바이트까지 같음을 확인했다. GitHub 러너에서 ModelScope에 닿는지(중국 호스팅)는 모른다. 느리거나 막히면 모델을 별도 저장소 릴리즈로 옮기고 sha256만 유지한다 |
| `wiki.db` 빌드 | 이 PC에서 24초에 만들고 `check_wikidb.py`를 통과(스키마 6, 학생 281 등)했다. 러너에서 SchaleDB와 arisu-archive에 닿는지는 모른다. 스토리 표가 비면 검사가 실패하게 했다 |
| 빌드 시간과 러너 자원 | 이 PC에서 테스트까지 컴파일 포함 1266초, 패키지 빌드 391초였다. 러너에서는 모르며 `scanner/target`을 캐시하지 않아 첫 실행이 느리다. `timeout-minutes: 90`으로 두었다 |
| Cloudflare Workers Builds | 연결, Root directory, 빌드 토큰의 D1 권한, `secrets.required`가 실제 배포에서 동작하는 것은 못 해 봤다. `wrangler deploy --dry-run`은 자격증명 없이 통과하지만 이 검사는 비밀값을 확인하지 않는다 |
| 마이그레이션 | 로컬 D1(`--local`)에 0001부터 0007까지 적용됨은 확인했다. 원격 적용은 못 했다 |
| 외부 사이트 | `npm run assets`를 다시 돌려야 하는 날에 SchaleDB와 두 위키가 그대로인지는 그때 알 수 있다 |
| 이미지 삭제 절차 | `git filter-repo` 명령은 실행하지 않았다. 옵션은 공식 문서로 확인했다 |

