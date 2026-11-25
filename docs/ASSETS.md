# 게임 이미지 에셋: 정책, 재생성, 삭제 요청 절차

## 정책

- `web/public/{portraits,icons,schools,card,card-lobby,halos,backgrounds}/`와 `web/public/{meta,halos,card-lobby,faces}.json`, `web/src/{catalog,backgrounds}.json`은 저장소에 커밋되어 있다(약 59 MB, 이미지 1417개). Cloudflare 빌드는 `npm ci`만 하고, 이 파일들을 그대로 올린다. 빌드에 Python이나 이미지 내려받기는 없다.
- 원본 입력인 `web/public/standing/`(38 MB), `web/public/lobby/`(2.9 MB), `web/scripts/.cache/`(455 MB)는 `web/.gitignore`로 제외한다. 카드 이미지 사슬은 `card-lobby` 다음 `lobby`, `card` 다음 `standing`, 마지막이 `portraits`인데 모든 학생(270명)에게 `card`와 `card-lobby`가 있어 앞의 두 단계가 항상 먼저 열린다. `standing`과 `lobby`는 파일이 빠졌을 때의 대비일 뿐이어서 커밋하지 않는다.
- 이미지는 `web/.gitattributes`로 binary 처리해 줄 끝 변환이 건드리지 않는다.

## 권리 표기와 요청 창구

게임 <블루 아카이브>의 이미지와 콘텐츠의 권리는 넥슨, 넥슨게임즈, Yostar에 있다. 이 프로젝트는 비상업 오픈소스 애드온이며 이미지는 SchaleDB와 위키에서 받은 것이다. 권리자의 요청이 있으면 이미지를 지운다. 요청은 저장소의 GitHub Issues(https://github.com/whyjp/ba_key_amas/issues)로 받는다.

## 다시 만들기 (유지보수자 전용)

`cd web && npm run assets`는 SchaleDB, bluearchive.wikiru.jp, bluearchive.wiki에서 전부 다시 내려받는다. 이 PC에서 처음부터 끝까지 1199초(약 20분)였고 내려받은 원본 캐시는 455 MB였다. 다시 쓰는 파일은 위 정책의 목록 전체다. 다시 만든 결과를 커밋하면 바뀐 파일 크기만큼 git 역사가 영구히 커진다. 카드 자르기 스크립트를 고친 경우처럼 꼭 필요할 때만 하고, 가능하면 바뀐 폴더만 한 번에 커밋한다.

변경이 PR에서 이미지를 5 MB 넘게 추가하거나 다시 쓰면 `scripts/release-check.ps1 -BaseRef`가 실패한다. 의도한 변경이면 PR에 `large-assets` 라벨을 붙인다.

## 삭제 요청을 받았을 때

새 커밋으로 파일을 지워도 이미지는 git 역사에 남는다. 아래 순서로 역사에서 없앤다. 표시: [되돌릴 수 없음], [사용자만 가능].

1. [사용자만 가능] 협업자에게 작업을 멈추라고 알린다. 열린 PR과 브랜치가 옛 역사를 되살릴 수 있다.
2. 새로 복제한 작업 사본을 만든다. `git filter-repo`는 새 복제본에서만 돌고, 이 PC의 로컬 저장소에는 쓰지 않는다.
   ```
   git clone --no-local <이 저장소의 경로 또는 https://github.com/whyjp/ba_key_amas.git> ba_key_amas-clean
   cd ba_key_amas-clean
   ```
3. 이미지 경로를 모든 역사에서 지운다. `--invert-paths`는 나열한 경로를 삭제한다(`--path`를 여러 번 쓰면 합집합).
   ```
   git filter-repo --invert-paths \
     --path web/public/portraits --path web/public/icons --path web/public/card --path web/public/card-lobby \
     --path web/public/halos --path web/public/backgrounds --path web/public/lobby --path web/public/schools \
     --path web/public/standing \
     --path web/public/halos.json --path web/public/card-lobby.json --path web/public/faces.json --path web/public/meta.json
   ```
   `web/src/catalog.json`과 `web/src/backgrounds.json`은 이미지가 아니라 학생 id와 이름 같은 자료이고 Worker가 가져다 쓴다. 요청이 이것까지 포함할 때만 `--path`로 더하고, 그러면 `index.ts`가 빌드되도록 빈 대체 파일(`{}`와 `[]`)을 커밋해야 한다. 민감 정보 제거 모드(`--sensitive-data-removal`)는 모든 ref를 먼저 가져오므로 다른 브랜치에도 있을 때 쓴다(공식 문서의 sensitive data 절).
4. 같은 경로를 `web/.gitignore`에 넣고 커밋해 다시 들어가지 않게 한다.
5. [되돌릴 수 없음, 사용자만 가능] filter-repo는 안전을 위해 `origin`을 지운다. 다시 연결하고 모든 ref를 강제로 덮어쓴다. GitHub의 `refs/pull/*`는 읽기 전용이라 실패하는 것이 정상이다.
   ```
   git remote add origin https://github.com/whyjp/ba_key_amas.git
   git push --force --mirror origin
   ```
   분기 보호가 걸려 있으면 잠시 풀고 다시 밀어야 할 수 있다.
6. [사용자만 가능] GitHub Support에 저장소 이름, 영향받은 PR 개수, filter-repo 출력의 "First Changed Commit(s)"를 알려 캐시된 화면, PR 참조, 서버 쪽 정리를 요청한다. 포크에는 남는다. 포크 소유자에게 지우라고 요청할 수밖에 없고, 다른 사람의 복제본은 이 쪽에서 지울 수 없다.
7. 작업 사본이나 다른 복제본에서 `git pull` 후 `git push`를 하면 옛 역사가 서버로 되돌아온다. 모두 새로 복제한다.
8. [사용자만 가능] 이미지 없이 Worker를 다시 배포한다. 정적 에셋에서 이미지가 사라지므로 해당 경로는 404를 돌려준다. 그때 화면이 어떻게 보이는지는 확인하지 않았다.

검증하지 못한 것: 위 명령은 이 저장소에 실행하지 않았다. 옵션의 뜻은 `git filter-repo`와 GitHub의 공식 문서로 확인했다.
