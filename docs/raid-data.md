# 레이드 데이터 (총력전, 종합전술시험, 월드 레이드 등)

위키 DB가 레이드류 전투에 대해 아는 것, 그 출처, 알 수 없는 것, 화면 판독기가 DB에 넘겨야 하는 것을 적는다.
스키마는 `wikidb/schema.sql`(버전 4)이다. 코드는 `rust/crates/wikidb/src/schaledb.rs`(수집기)와
`rust/crates/wikidb/src/raid.rs`(조회)에 있다.

## 모델

원본(SchaleDB `raids.json`)에는 전투 종류가 다섯 가지 있고 종류마다 모양이 다르다. 수집기는 경계에서 이를 한 번
파싱해 같은 테이블 네 개로 넣는다. 그 뒤로는 원본 JSON을 읽는 곳이 없다.

| 테이블 | 한 행이 가리키는 것 | 비고 |
| --- | --- | --- |
| `raid` | 전투 하나, 키 `(kind, id)` | `kind`는 `Raid`(총력전), `TimeAttack`(종합전술시험), `WorldRaid`, `MultiFloorRaid`, `InteractiveWorldRaid`이다. id는 종류가 달라도 겹친다. `raw`에 원본 기록을 보관한다. |
| `raid_difficulty` | 전투의 난이도 하나, 키 `(raid_kind, raid_id, idx)` | `idx`는 원본 배열의 위치이다. 레이드가 Global에 출시됐고 `idx`가 Global의 `MaxDifficulty` 안이면 `released_global`이 참이다. |
| `raid_difficulty_enemy` | 난이도의 적 하나, 원본 순서 | `enemy`로 연결된다. 소환물, 투사체, 호위 수송차도 여기에 있다. `enemy.rank`로 거른다. |
| `raid_season` | Global 시즌 하나, 키 `(mode, season_id)` | `mode`는 `Raid`(총력전) 또는 `Eliminate`(대결전)이다. 시즌의 `terrain`은 그 레이드를 치르는 지형이다. `start_at`과 `end_at`은 유닉스 초이다. |

`v_raid_difficulty`는 난이도에 대표 적(이름, 공격 타입, 방어 타입)을 붙인 뷰이다.

### 컬럼 설명

- `raid.name_ko`는 원본의 이름이다. 원본이 이름을 주지 않으므로 `TimeAttack`은 전부 NULL이다.
- `raid.display_ko`는 화면에 보여 줄 이름이자 대조할 이름이다. `name_ko`이고, 종합전술시험은 기록에서 만든 이름이다.
  시험, 지형, 적 방어 타입을 이어 붙이며 예를 들면 `사격 시험 · 야외 · 경장갑`이다. 이렇게 만든 이름이 게임에 실제로
  나오는 문자열인지는 확인되지 않았다. 실제 문구를 보는 화면 판독기는 이 이름과 비교해야 한다.
- `raid.terrain`은 그 레이드를 치를 수 있는 지형의 목록이다. 정답이 아니라 후보 목록이고, 정답은 시즌의 지형
  (`raid_season.terrain`)이다.
- `raid`에는 공격 타입과 방어 타입이 더 이상 없다. 두 타입은 적마다 다르고 난이도에 따라 바뀐다. 예전의 단일
  `raid.bullet_type`과 `raid.armor_type`은 Normal 난이도의 값만 담고 있어서 Insane 이상에서는 틀렸다(비나는 Insane부터
  Pierce인데 컬럼에는 Normal 값이 들어 있었다).
- `raid_difficulty.label`은 원본의 이름이다. `Raid`는 `Normal`, `Hard`, `VeryHard`, `HardCore`, `Extreme`, `Insane`,
  `Torment`, `Lunatic`이며 위치로 정한다(아래 참고). 월드 레이드는 `DifficultyName`이다(상호작용형은 `A`..`D`이고
  월드 레이드 하나는 `HardCore`에서 시작한다). 종합전술시험과 층 레이드에는 없다.
- `raid_difficulty.level`은 원본이 적 레벨을 줄 때(종합전술시험, 월드 레이드) 그 값이다. 종합전술시험의 레벨은
  25, 40, 65, 78 네 개이고 이름은 없다.
- `raid_difficulty.floor_start`는 `MultiFloorRaid` 단계의 첫 층이다.
- `raid_difficulty.boss_enemy_id`는 Boss 랭크 적 중 첫 번째, 없으면 Champion 중 첫 번째, 그것도 없으면 NULL이다
  (방어형이나 파괴형 종합전술시험에는 둘 다 없다).

### 수집기가 읽는 종류별 모양

| 종류 | 난이도의 출처 | 라벨 |
| --- | --- | --- |
| `Raid` | `EnemyList[idx]`, `BattleDuration[idx]` | `Normal, Hard, VeryHard, HardCore, Extreme, Insane, Torment, Lunatic` 중 위치로 정함 |
| `TimeAttack` | `Formations[idx].EnemyList`, `Level[idx]`, `BattleDuration[idx]` | 없음 |
| `WorldRaid`, `InteractiveWorldRaid` | `EnemyList[idx]`, `Level[idx]`, `BattleDuration[idx]` | `DifficultyName[idx]` |
| `MultiFloorRaid` | `EnemyList[tier]`, `DifficultyStartFloor[tier]`, `BattleDuration` 하나 | 없음 |

`Raid`의 라벨 순서는 추론이다. SchaleDB의 `RaidDifficulty` 현지화에 이 이름들이 나열되어 있고, 적의 공격 타입이
위치 5에서 바뀌며 거기서부터 기록의 `BulletTypeInsane`이 적용된다. 이 순서는 테스트로 고정해 두었다.

## 공격 타입에 대해 데이터가 말하는 것

공격 타입과 방어 타입은 레이드의 속성이 아니라 난이도에 속한 적마다의 속성이다. 보스의 공격 타입은 난이도에 따라
바뀐다. 비나는 위치 0부터 4까지 Normal이고 5부터 7까지 Pierce이며, 예로니무스는 Normal 다음에 Explosion이다.
졸개와 소환물은 보스와 다를 수 있다(예로니무스의 유물은 Normal로 남는다). 임무 구조체는 방어 타입과 공격 타입을 적
HP의 비중으로 받는다(`v_raid_armor_share`, `v_raid_attack_share`. `docs/enemy-weighting.md` 참고). `Summoned`,
`Student`, `DestructibleProjectile` 랭크는 가중치가 0이다.

## 조회 API (`baocr_wikidb`, 모듈 `raid`)

함수는 모두 `Wiki`에 있고, DB에 대해 순수하며, 시각을 인자로 받는다.

- `raids_by_name(name) -> Vec<RaidCandidate>`. 이름이 `name`과 같은(공백과 구두점은 무시) Global 출시 레이드를
  돌려준다. 같은 것이 없으면 `name`으로 시작하는 것들을 돌려준다. 후보를 여럿 돌려주며 첫 번째만 고르지 않는다.
  "비나"는 `Raid`이면서 `InteractiveWorldRaid`이고 "게부라"도 그렇다. 이름이 종합전술시험과 같을 수도 있다. 후보는
  종류, id 순으로 정렬한다. `exact`는 전체 일치와 접두 일치를 구분한다.
- `raid_difficulties(kind, id) -> Vec<RaidDifficulty>`. 전부 돌려주며 닫힌 것은 `released_global`로 표시한다.
- `raid_seasons(raid_id)`와 `current_or_nearest_season(raid_id, now) -> Option<SeasonPick>`. `now`에 진행 중인
  시즌을 돌려준다. 없으면 시간상 가장 가까운 시즌을 돌려주고 `relation`은 `Active`, `Upcoming`, `Past` 중 하나다.
  같은 거리면 끝난 쪽을 고르고, 동시에 여럿이 진행 중이면 가장 늦게 시작한 것을 고른다. 시즌이 있는 것은 총력전뿐이다.
- `raid_mission(kind, id, idx, season) -> Option<MissionBrief>`. 기존 임무 구조체를 그대로 쓰므로
  `formation::evaluate`는 바뀌지 않는다. `stage_id`는 레이드 id, `difficulty`는 난이도 `idx`, `terrain`은 시즌의
  지형(없으면 레이드의 유일한 지형, 그것도 없으면 미상), `recommended_level`은 적 레벨이다.
  구조체를 확장할 필요가 없었다. 방어 타입별 비중과 공격 타입별 비중에 레이드가 가진 정보가 모두 담긴다.

## 알려진 공백

- 다중 부대 규칙. 총력전은 부대를 여럿 쓴다(시도마다 스트라이커 부대 하나, 부대끼리 달라야 하는 보스). SchaleDB에는
  이런 정보가 없다. `raid_mission`으로 계산한 판정은 부대 하나를 판정한다.
- 대결전의 방어 타입 선택. `raid_season.open_difficulty`(모드 `Eliminate`)에는 방어 타입별로 열려 있는 최고 난이도가
  들어 있다. 적 데이터에는 보스마다 방어 타입이 하나뿐이고, 방어 타입 세 가지로 도는 판이 적 기록에 어떻게 대응하는지는
  모델링하지 않았다. 대결전 시즌의 `raid_mission`은 적 기록을 있는 그대로 쓴다.
- 종합전술시험은 원본에 이름이 없고 게임의 문구도 모른다. 여러 종합전술시험이 같은 표시 이름(같은 시험, 지형, 방어
  타입)을 가진다. 이런 중복은 적이 똑같고 전투 시간과 Global 출시 여부만 다르다. `raids_by_name`이 같은 표시 이름의
  후보를 여럿 돌려줄 때 그 임무들은 같다.
- 종합전술시험 규칙(`TimeAttackRules`), 보상, 스킬, HP와 공격력 능력치는 `raw`에만 있다.
- 층 레이드. 마지막 단계의 끝 층은 모델링하지 않았다(`DifficultyStartFloor`의 경계가 단계 수보다 하나 많다).
- 시즌과 난이도의 이용 가능 여부는 SchaleDB 스냅샷 기준이다. 그 뒤 게임에 추가된 시즌은 DB를 다시 만들 때까지
  없다.

## 화면 판독 단계가 줘야 하는 것

조회를 쓰려면 판독기가 다음을 넘겨야 한다.

1. 게임이 보여 주는 보스 이름 텍스트(OCR). 이것이 `raids_by_name`으로 간다. 종합전술시험이면 시험, 지형, 방어 타입
   문구를 대신 넘긴다.
2. 어떤 종류의 화면인지(총력전, 종합전술시험, 월드 레이드 등). 이름이 같은 후보를 가르는 데 쓴다.
3. 난이도. 게임의 난이도 이름이나 번호로 준다. 총력전에서는 `RaidDifficulty::label`(`Normal` .. `Lunatic`)로 `idx`에
   대응시킨다. 난이도가 없으면 공격 타입은 난이도별로만 줄 수 있다.
4. 지형을 정할 시즌(게임이 보여 주는 번호 또는 현재 시각). `now`가 `current_or_nearest_season`으로 간다. 관계가
   `Past`나 `Upcoming`이면 화면이 다른 시즌의 것일 수 있다. 화면에 지형이 보이면 시즌의 지형보다 화면의 것을 우선한다.
5. 대결전이면 지금 보이는 판의 방어 타입.

`Wiki::open`이 스키마 버전 불일치를 알리면 DB를 다시 만든다(`cargo run -p baocr-wikidb --release`).
`cargo run -p baocr-wikidb --example raid_report -- <wiki.db>`는 행 수를 출력하고 불변식을 검사한다.

## 레이드 화면

애드온이 화면에서 무엇을 읽고 위 조회에 어떻게 닿는지 적는다. 코드는 `extract/src/scene.rs`(화면),
`extract/src/raid.rs`(화면 텍스트에서 전투 찾기), `assist/src/raid_lookup.rs`(화면에서 `MissionBrief`로),
`assist/src/widgets.rs`(`RaidInfo` 카드, 편성 판정)에 있다. 모두 `fixtures/screens/`의 픽스처(`*.boxes.txt` OCR
박스, 일부는 PNG)에서 읽는다. 라이브 하네스([live-e2e.md](live-e2e.md)의 시나리오 `nav`)는 게임 안에서 총력전, 모의
보스 목록, 방 정보, 모의 편성 화면도 거친다. 4:3 프레임만 다룬다.

| 화면 | 화면 종류 | 얻는 것 | 근거 |
| --- | --- | --- | --- |
| 업무(모든 모드) | `WorkHub` | 없음 | `work-hub-full` |
| 총력전 "Battle Select" | `RaidSelect` | 보스, 난이도 행 Normal..Extreme | `raid-select`(png: `raid-select-hieronymus`) |
| 모의 총력전 보스 목록 | `MockRaidBosses` | 보스, 방어 타입 단어(이중 방어는 둘로 센다), 보스의 지형 탭 둘 | `raid-mock-m00..m14`(보스 14개, m14는 m00과 같다) |
| 방 정보 팝업 | `RaidRoomInfo` | 난이도, 보스, 레벨, 공격, 방어 | `raid-mock-room-info` |
| 총력전, 모의 총력전, 제약해제결전의 편성 | `RaidFormation(mode)` | 모드(헤더에서만). 화면에 보스와 학생 이름은 없다 | `raid-formation-empty`, `unrestricted-raid-formation` |
| 제약해제결전 선택, 단계 구간, 스테이지 하나 | `UnrestrictedRaidSelect`, `Stages`, `Stage` | 보스. 보스, 레벨, 공격, 방어(스테이지 화면) | `unrestricted-raid-select`, `-stages`, `-stage01` |
| 종합전술시험, "다음 시즌 정보" 팝업 | `TimeAttackSelect`, `TimeAttackNextSeason` | 화면 종류 라벨뿐 | `timeattack-select`, `timeattack-next-season` |
| 시즌 사이의 대결전 | `EliminateClosed` | 화면 종류 라벨뿐 | `eliminate-closed` |
| 제약해제결전 진입 전 경고 | `SpoilerWarning` | 화면 종류 라벨뿐 | `spoiler-warning` |

`extract`는 헤더 텍스트와 그 화면에만 있는 단어(예: "Battle Select", "보스 선택", 팝업 제목 "방 정보"와 "다음 시즌 정보")로
화면 종류를 가르며, 위치만으로 가르지 않는다. 편성의 모드는 헤더(`총력전 편성`, `모의 총력전 편성`, `제약해제결전 편성`)에서
얻는다. 모의 화면에도 "총력전"이 옆 버튼으로 나오기 때문이다.

### 화면 텍스트에서 카드까지

1. `RaidTracker`가 목록, 팝업, 스테이지 화면에서 전투(`RaidRef`: 모드, 보스 텍스트, 난이도 위치, 방어 타입 단어, 공격
   단어, 레벨, 지형 탭)를 기억한다. 편성 화면에는 이런 정보가 없으므로 기억해 둔 것을 쓴다. 임무 화면은 레이드를 잊고
   레이드 화면은 임무를 잊는다. 편성의 모드는 기억한 전투와 맞아야 한다.
2. `raid_lookup::lookup`은 모드의 종류에 속한 레이드 중에서 보스 이름으로 레이드를 찾는다(총력전과 모의 총력전은
   `Raid`, 제약해제결전은 `MultiFloorRaid`). 방어 타입과 공격 타입 단어는 `label` 표(종류별 `Wiki::decode_labels`)로
   해독한다. 일반은 공격 타입이고 일반장갑은 방어 타입이다. 표에 없는 단어는 추측하지 않고 알린다. 난이도 단어는
   위치로 `idx`에 대응시킨다(Normal 0 .. Lunatic 7. 게임은 `Hardcore`로 쓰고 DB는 `HardCore`로 쓴다). 지형은 총력전
   시즌에서 가져온다(`Raid` 모드 시즌만. 대결전 시즌은 무시한다). 그 뒤 `raid_mission`으로 브리프를 만든다.
3. 값은 화면이 아니라 위키 DB가 준다. 화면에 보스의 방어 타입, 공격 타입, 레벨이 나와 있으면
   `raid_lookup::cross_check`가 DB와 비교만 하고 아무것도 바꾸지 않는다(방어 타입은 위키의 구성에 없는 것이 화면에
   보일 때만 알린다). 차이는 카드에 메모("화면 ... / 위키 ...", 숫자는 위키에서 계산한 값)로 보인다.
   [ADR 0005](adr/0005-static-db-dynamic-screen.md)를 참고한다.
4. 조용히 고르는 일은 없다. 이름이 같은 레이드가 여럿이면 화면의 방어 타입으로 가려지지 않는 한 `Ambiguous`로 남는다
   (두 세트의 분노는 방어 타입이 다르고 경장갑이 id 1000001이다). 난이도가 나오지 않는 화면(보스 목록)에는 Normal을
   주지 않는다. 카드에는 "선택 전"이라고 쓰고 난이도 구간별 공격 타입을 나열하며, 편성 카드에는 판정에 쓴 난이도를
   밝힌다.

### 데이터에서 확인한 것

- 모의 보스 14개에서 읽은 방어 타입은 13개에서 Normal 대표 적의 DB 방어 타입과 같다. 드럼통게는 `특수장갑/경장갑`으로
  보이는데 DB의 대표 적은 `Unarmed` 하나뿐이다. 다른 적(파손된 선체, 강철 집게)은 `LightArmor`이므로 이중 방어는
  보스와 그 부위를 합친 것이고, 화면은 전체 적 명단과 일치한다. 실제 DB로 확인했다.
  `BAOCR_WIKI_DB=<wiki.db> cargo test -p baocr-assist real_database -- --nocapture`.
- `raid_seasons`는 같은 레이드 id의 대결전 시즌도 돌려준다. 총력전에는 `SeasonMode::Raid`만 쓴다.

### 편성 카드

편성 위젯은 레이드의 브리프 위에서 기존 위젯을 그대로 쓴다. 총력전과 모의 총력전은 부대를 최대 넷(1부대 .. 4부대)
쓰며, 카드는 이를 알리고 판정이 마지막으로 읽은 부대 하나를 판정한다고 알린다. 편성 화면에는 학생 이름이 나오지
않는다(제약해제결전 화면은 초상화와 레벨을 보여 준다). 그래서 임무에서와 같이 빠른 편성 팝업으로 편성을 읽는다.
어느 부대 탭을 편집 중인지는 읽지 않는다. 다중 부대 최적화는 없다.

### 다루지 않는 것

- 시즌이 진행 중인 대결전(픽스처가 없다. 닫힌 헤더가 아닌 대결전 헤더는 모두 `EliminateSelect`가 되며 검증하지 못함)과
  그 방어 타입별 판. 전술 대회(PvP). 종합전술시험과 대결전에는 카드가 없고 화면 종류 라벨만 있다.
- 접힌 부분 아래에 있는 총력전 목록의 Insane, Torment, Lunatic 행(목록이 스크롤된다). 모의 총력전 목록은 화면에 있는
  행까지만 읽는다. 제약해제결전 단계 구간에서 고른 행(텍스트가 아니라 강조 표시)과 그 스테이지의 지형(아이콘). 보스의
  지형 탭 둘 중 어느 쪽이 강조되었는지(색).
- 실제 총력전 입장 팝업과 총력전 편성(기록하지 않았다. `총력전 편성`을 헤더로 분류하는 것은 추론이다).
- 레이드 편성 위에 열린 빠른 편성은 기록하지 않았다. 학생 격자는 임무의 것과 같다고 가정한다(아이콘을 그린다). 그
  아래 슬롯 줄은 레이드에서 어떤 모양인지 모르므로 슬롯 아이콘이 꺼진다.
- 4:3이 아닌 프레임.
