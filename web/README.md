# ba-key-AMAS 웹

스캐너(`../scanner`)가 만든 유저 데이터를 올려 두고, 키로 다시 꺼내 보는 Cloudflare Workers 앱이다. 스캐너 코드와는 폴더도 빌드도 따로다.

| 구성 | 선택 | 이유 |
|---|---|---|
| 런타임 | Workers + 정적 에셋 | 서버 관리가 없고, 서버 쪽 의존성은 0개다. 런타임 패키지가 없어 공급망 공격면이 작다 |
| DB | D1 (SQLite) | JSON 몇 개 분량이라 무료 한도 안이다. 학생 표를 SQL로 정렬 가능한 컬럼으로 둔다 |
| 화면 | 프레임워크 없는 HTML, CSS, JS | 의존성이 없고, `innerHTML`을 쓰지 않는다 |
| 개발 도구 | wrangler, typescript | `devDependencies`만, 정확한 버전으로 고정 |

## 키

- 키는 `닉네임#6자리hex`다. 닉네임은 스캐너가 읽은 이름이고, 대소문자와 전각을 접어서 중복을 막는다.
- 서버는 키 원문을 저장하지 않는다. `KEY_PEPPER` 비밀값으로 만든 HMAC-SHA256만 저장한다.
- 키는 URL이 아니라 POST 본문으로만 보낸다. 로그와 Referer에 남지 않는다.
- 닉네임만으로는 조회할 수 없다. 틀린 키와 없는 닉네임은 같은 응답이다.
- `POST /api/rekey`로 키를 바꾸면 이전 키는 바로 무효다. `POST /api/delete`로 데이터를 지운다.
- 6자리 hex는 2^24가지라 추측 공격에 약하다. 막는 수단은 두 겹이다. 한 닉네임에 5번 틀리면 15분 잠기고, IP당 분당 30건의 속도 제한(`LIMITER`)이 건다. 잠금은 남이 닉네임을 알면 본인도 15분간 못 들어가게 만들 수 있는 대가가 있다.

## 화면

| 경로 | 내용 |
|---|---|
| `#/` | 랜딩. 업로드, 내 정보, 리더보드 세 입구 |
| `#/upload` | 파일을 골라 올린다. 닉네임과 리더보드 공개 여부를 정한다. 새 키는 한 번만 보여 준다 |
| `#/me` | 키를 넣으면 들어간다. 탭은 요약, 대표 3인, 학생 타일, 학생 리스트, 스토리, 이벤트, 임무, 모모톡, 당번 · 메모리얼, 스케줄 |
| `#/board` | 공개를 고른 유저의 순위. 열 머리글을 눌러 정렬한다 |

스캔하지 않은 데이터(스토리, 임무)는 탭에 "아직 스캔한 기록이 없습니다"로 나온다.

## 애드온에서 올리기

```
ba-key-AMAS --upload https://<서버> [--public] [--nickname 이름]
```

마지막 학생 스캔(`students.json`)과 진행 스캔(`latest.json`)을 보낸다. 닉네임은 진행 스캔이 읽은 계정 이름이다. 서버가 준 키는 `%APPDATA%/baocr/upload.json`에 서버와 닉네임별로 저장해, 다음 업로드가 같은 항목을 갱신한다. 키가 새어 나가지 않도록 https와 loopback 주소만 받는다.

## API

`GET /api/leaderboard`를 빼고는 모두 `POST`, `Content-Type: application/json`, 본문 3 MB 이하다.

| 경로 | 본문 | 응답 |
|---|---|---|
| `/api/upload` | `{nickname, key?, public?, roster, progress?}` | 새 닉네임이면 201과 새 키. 있는 닉네임은 맞는 `key`가 있어야 갱신하고, 없으면 409. `public`은 기본이 꺼짐 |
| `/api/profile` | `{key}` | 닉네임, 계정, 학생 목록, 진행 상태, 점수 분석(캐릭터별 합산과 대표 3인) |
| `/api/visibility` | `{key, public}` | 리더보드 공개 여부 변경 |
| `/api/rekey` | `{key}` | 새 키 |
| `/api/delete` | `{key}` | 삭제 |
| `GET /api/leaderboard` | 없음 | 공개 유저 100명까지, 점수 순. 60초 캐시 |

`roster`는 스캐너의 `students.json` 그대로다. 서버가 알려진 필드만 범위를 검사해서 남기고 나머지는 버린다. 메모리얼 로비 해금은 `progress.duty.students`의 이름과 학생 이름을 맞춰 가져온다.

## 점수

`src/score.ts`가 계산하고 `test/score.test.ts`가 고정한다. 기준은 게임 공식이 아니라 노력의 양을 가늠한 값이다.

- 캐릭터는 이름에서 `(의상)`을 뺀 이름이다. 의상이 다른 학생의 점수는 더하고, 화면에는 의상을 모두 나란히 보인다.
- 인연은 30까지 한 랭크 2점, 31에서 59까지 3점, 60에 닿으면 100점을 더하고 그 위는 6점이다.
- 고정 40점, 메모리얼 로비 50점이다.
- 육성(레벨, 성급, 스킬, 고유무기)은 0.6배다. 고유무기는 가진 것만으로 30점에 성급당 15점, 레벨당 0.6점이라 육성에서 가장 크다.
- 리더보드 열은 인연 60+, 인연 30+, Lv 80+, 고정, 메모리얼 캐릭터 수와 보유 캐릭터 수다. 던전, 전투, 레이드는 넣지 않았다.
- 점수는 업로드할 때 계산해 저장한다. 기준을 바꾸면 `SCORE_VERSION`을 올리고 유저가 다시 올려야 새 기준이 적용된다.

## 보안 설정

- 화면과 에셋은 `public/_headers`의 CSP(`default-src 'none'`, 스크립트와 스타일은 `'self'`만), `X-Frame-Options`, `nosniff`, `no-referrer`가 붙는다. API 응답은 `Cache-Control: no-store`다.
- 상태를 바꾸는 요청은 Origin이 다르면 거부하고, JSON이 아니면 거부한다(CSRF). 쿠키는 쓰지 않는다.
- SQL은 모두 바인딩 인자로 쓴다.

## 개발

```
npm ci
npm run assets          # 학생 카탈로그와 초상화를 SchaleDB에서 받는다 (커밋하지 않는다)
echo KEY_PEPPER=dev-only > .dev.vars
npm run db:local
npm run dev
node scripts/upload-local.mjs http://127.0.0.1:8787 소다팝
npm test ; npm run typecheck
```

## Cloudflare 깃 연결 배포

1. `npx wrangler d1 create ba-key-amas`를 한 번 실행하고 나온 ID를 `wrangler.jsonc`의 `database_id`에 넣는다.
2. 대시보드에서 Workers Builds로 이 저장소를 연결하고 Root directory를 `web`으로 한다.
3. Build command는 `npm ci && npm run assets`, Deploy command는 `npx wrangler d1 migrations apply ba-key-amas --remote && npx wrangler deploy`로 한다. 마이그레이션은 자동으로 돌지 않아서 배포 명령에 넣었다.
4. 비밀값 `KEY_PEPPER`는 대시보드의 Variables and secrets에 Secret으로 넣는다. 바꾸면 기존 키가 모두 무효가 되므로 한 번 정하고 유지한다.
5. 초상화와 `src/catalog.json`은 저장소에 없고 빌드 때 만든다. 게임 이미지를 저장소에 두지 않기 위해서다.

---

게임 <블루 아카이브>의 에셋 및 콘텐츠의 권리는 넥슨, 넥슨게임즈 및 Yostar에 있습니다.
