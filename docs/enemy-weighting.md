# 적 방어 타입과 공격 타입의 가중

문제: 임무나 레이드에는 방어 타입과 공격 타입이 서로 다른 적이 섞여 나오는데, 그 비중이 같지 않다.
머릿수로 세면 HP 1인 졸개와 보스를 똑같이 취급한다. 드럼통 게는 화면에 특수장갑과 경장갑을 함께 보여 주지만
DB의 대표 적은 특수장갑 하나뿐이다. 이 문서는 자료가 무엇을 뒷받침하는지, 데이터가 무엇을 주는지, 앱이 어떤
모델을 쓰고 왜 그것을 골랐는지 적는다. 근거는 출처를 달아 인용하고, 추론은 추론이라고 표시한다.

표시: **DATA**는 우리가 SchaleDB의 JSON이나 JS를 직접 읽은 것이다. **GUIDE**는 커뮤니티나 위키 페이지를 fetch로
받아 요약한 것이며 원문 그대로가 아니다. **INFERENCE**는 우리의 추론이다.

## 정적 데이터와 동적 데이터

던전과 학생 판정은 정적 DB에서 나온다. 화면이 알려 주는 것은 플레이어가 어느 전투를 골랐는지뿐이다.

| 항목 | 정적 / 동적 | 출처 | 쓰는 곳 |
| --- | --- | --- | --- |
| 어느 화면인지, 모드, 임무나 보스의 이름과 번호, 난이도 | 동적 | 화면 판독기(`extract`) | `assist::observe`와 `raid_lookup::lookup`의 조회 키 |
| 편성에 든 학생(이름, 슬롯), 선택 | 동적 | 화면 판독기 | `Wiki::match_student`, 편성 위젯 |
| 적의 방어 타입과 공격 타입 구성(비중) | 정적 | `v_stage_*_share`, `v_raid_*_share`(`*_enemy_weight`에서 만듦) | `MissionBrief.armor/attack`, `formation::evaluate`, 상성표 |
| 적 명단, 레벨, HP, 가중치 | 정적 | `formation_enemy_weight`, `raid_difficulty_enemy_weight` | 비중 뷰, `examples/weight_report.rs` |
| 스테이지 지형, 권장 레벨 | 정적 | `stage.terrain`, `stage.level`, `raid_season.terrain`, `raid_difficulty.level` | `MissionBrief` |
| 학생의 공격 타입, 방어 타입, 지형 적응, 역할 | 정적 | `student` | `StudentBrief`, `fit_tier` |
| 타입 상성 | 정적 | `type_effectiveness` | `TypeTable` |
| 레이드 화면의 방어 타입과 공격 타입 단어 | 동적, 교차 확인용 | 화면 판독기(`RaidScreenWords`) | `raid_lookup::cross_check`, 진단용 행이며 값으로 쓰지 않는다 |

## 자료가 말하는 것

1. 일반 스테이지에서 보스와 졸개가 섞인 무리를 어떻게 셀지 정한 규칙은 열어 본 자료 어디에도 없다
   (GUIDE game8 `https://game8.jp/blue-archive/656430`: 콘텐츠의 방어 타입을 확인하고 효과적인 공격 타입으로
   편성하라는 내용이며 가중에 관한 언급은 없다). 머릿수, HP, 웨이브에 관한 규칙은 찾지 못했다.
2. 총력전은 시간으로 점수를 매긴다(GUIDE `https://bluearchive.wiki/wiki/Total_Assault`: "based on total time spent in
   combat"). 보스는 하나의 큰 HP 풀이므로 점수에 반영되는 HP 대부분에 보스의 방어 타입이 배율을 정한다
   (INFERENCE).
3. 부위와 소환물은 균일하지 않다. GUIDE(레이드 공략 문서,
   `https://docs.google.com/document/d/1bvYGwIh-9o-2JQ4Q2MQXtsL0peiCPrRuw0l3HQmTrZ0`)에 따르면 그레고리우스의 성가대에
   준 피해는 "will be reflected onto Greg"이고, 호버크래프트 MGS에 준 피해는 "will be reflected onto the Hovercrafts
   HP"이며, 게부라의 방열기는 "cannot be damaged by your team"이다. 부위는 보스마다 보스 HP 바에 반영되기도 하고,
   방해물이기도 하고, 무시해도 되기도 한다.
4. 드럼통 게(DATA `https://schaledb.com/data/kr/raids.min.json`, `enemies.min.json`): 레이드 기록에
   `ArmorType: Unarmed`와 `SubArmorType: LightArmor`가 있다(보조 방어 타입이 있는 레이드는 이것뿐이다). 실제 레이드
   카드도 둘 다 표시한다(DATA `https://schaledb.com/assets/`의 `TimeAttackCard-*.js`). `PhaseChange`는 Boss 유닛에만
   있고(Normal에서 HP 400000 중 200000 미만), 개발용 이름은 `EN0022_TotalHP_...`이다. "(1 페이즈)", "(2 페이즈)",
   배딱지, 부패한 드럼통 항목은 Boss와 같은 HP 400000을 적어 두었다. 부위는 자기 HP를 따로 가진다. 집게발 3000(특수),
   선체 25000(경장갑), 강철 집게 200000(경장갑)이다. 스킬 설명은 "배딱지, 집게발, 선체로 구성되며, 각각 개별적인 방어 타입"
   이다. GUIDE(ruliweb `https://bbs.ruliweb.com/etcs/board/185438/read/1398296`,
   `https://bluearchive.wiki/wiki/Drumbarka`)에 따르면 1페이즈 몸통은 특수(기본 목표)이고 선체는 파괴할 수 있는
   경장갑이다. 2페이즈 강철 집게는 경장갑이며 EX로만 공격한다. 따라서 두 방어 타입 모두 실제로 상대하게 된다.
5. 페이즈: 1페이즈가 HP의 약 55 %를 차지한다(`PhaseChange` 기준값, DATA). 1페이즈는 약 2분, 2페이즈는 약 1분 30초
   걸린다(GUIDE ruliweb). 부위별 소요 시간을 수치로 낸 자료는 없다.
6. 공격 타입: Normal 난이도의 공격은 모든 학생 방어 타입에 1.0배다(GUIDE game8). Insane 이상에서는 보스의 공격
   타입이 바뀐다(DATA `BulletTypeInsane`. 비나와 드럼통 게는 관통, 호드는 신비. GUIDE bluearchive.wiki의 비나와 호드
   페이지).
7. 능력치: 랭크 배율은 없다. 적마다 `MaxHP1`과 `MaxHP100`이 있고, 레벨 L의 능력치는
   `ceil(round(s1 + (s100 - s1) * round4((L-1)/99)) * grade multiplier)`이다. 등급 배율은
   `1 + sum(HP steps[0..grade]) / 10000`이고 기본 단계는 `[0, 500, 700, 900, 1400]`이다. 스테이지 적의 레벨은
   `Level<Rank>`가 있으면 그 값, 없으면 `LevelMinion`이며, 등급도 `Grade<Rank>`가 있으면 그 값, 없으면
   `GradeMinion`이다(DATA, 현행 번들 `https://schaledb.com/assets/index-c2394870.js`, `RaidView-*.js`). 레이드 레벨은
   난이도별로 기본 `[17,25,35,50,70,80,90,90]`이다(같은 번들). 레이드 HP는 난이도마다 고정값으로 정해져 있다
   (`MaxHP1 == MaxHP100`).

확정하지 못한 것: 드럼통 게 부위에 준 피해가 메인 HP 바도 깎는지, 선체를 늘 겨냥할 수 있는지 EX로만 겨냥할 수 있는지,
`ArmorTypeRestriction`의 뜻, 부위별 소요 시간 자료, 일반 스테이지의 혼합 구성에 대한 커뮤니티 규칙.

## 데이터가 주는 것

스테이지 편성(formation)은 차례로 싸우는 맵 유닛이며 웨이브는 `stage_formation.ordinal`이다. 적의 `Rank`는 Minion,
Elite, Champion, Boss, Summoned, DestructibleProjectile, Hallucination, Student이다. 적별 HP, 공격력, 레벨은 원본에
있고 적에게는 `TacticRole`이 없다. 스테이지 기록의 `ArmorTypes`는 가중이 아니라 집합이다. 스테이지 340개, 편성
1325개 중 13개 스테이지에서 적 한 마리가 스테이지 HP의 90 %를 넘는다. 현상수배 ChaserE에는 HP 70,012,000인 챔피언
(센티널급 값)이 HP 77,000인 적과 HP 약 200인 졸개 옆에 있다.

## 실제 행으로 본 세 모델

모델 A는 머릿수 세기(기존 규칙)다. 모델 B는 HP 가중이다. 적마다 전투의 레벨과 등급에서 나온 HP를 더하고, 소환물과
호위 수송차와 사격 가능한 투사체는 0으로 센다. 모델 C는 B에 레이드용 구조 규칙 하나를 더한 것이다. 보스가 아닌데
HP가 보스와 정확히 같은 항목은 그 보스의 몸체이므로 0으로 센다(페이즈, 스킨, 공유 HP 바). 역할에 곱하는 수치 배율은
없다. 뒷받침하는 자료가 없기 때문이다.

| 사례(DB 행) | A 머릿수 | B HP | C 채택 |
| --- | --- | --- | --- |
| 드럼통 게 Normal | 특수 78 / 경 22 | 특수 91 / 경 9 | 특수 64 / 경 36 |
| 드럼통 게 Insane | 특수 78 / 경 22 | 특수 93 / 경 7 | 특수 68 / 경 32 |
| 비나(보스 하나, 모든 난이도) | 중 100 | 중 100 | 중 100(공격 일반, Insane 관통) |
| KV 하이웨이 35번 도로(Champion 중장갑, 엘리트 2, 경장갑 졸개 2) | 중 40 / 경 60 | 중 80 / 경 20 | B와 같음(스테이지에는 미러가 없다) |
| (구) 사립 히아신스 학원: 웨이브 1과 2는 엘리트와 졸개가 특수, 웨이브 3 엘리트는 경, 웨이브 4 챔피언은 경 | 특수 48 / 경 52 | 특수 17 / 경 83 | B와 같음 |
| 현상수배 ChaserE Lv.10(경장갑 챔피언 2, 중장갑 졸개 6) | 경 25 / 중 75 | 경 100 / 중 0 | B와 같음 |

읽는 법: A는 떼를 과대평가한다. ChaserE는 적 대부분이 중장갑이라고 나오지만, 편성이 실제로 상대하는 것은 경장갑
챔피언이다. B는 보스와 보스 웨이브를 따라간다. 다만 드럼통 게에서는 공유 HP 바 400k를 여섯 번 센다(Boss, 몸체 둘,
배딱지 둘, 부패한 드럼통). 그래서 공략 문서가 실제로 상대한다고 하는 경장갑 부위가 묻힌다. C는 HP 바를 한 번만 세고
부위는 자기 HP대로 남긴다. 그래서 DB만으로 화면과 같이 두 방어 타입이 모두 나온다. C를 채택한다.

한계. HP 비중은 시간 비중이 아니다. 범위 공격은 떼를 한꺼번에 처치하는데 B와 C는 이를 반영하지 않으므로 ChaserE의
중장갑 0 %는 하한이다. ChaserE의 70M 센티널 값은 그 스테이지의 방어 타입을 바꾸지 않는다(챔피언 둘 다 경장갑이다).
미러 규칙은 드럼통 게 데이터와 `TotalHP`라는 개발용 이름에서 끌어낸 추론이며 드럼통 게에만 적용한다. 카이텐저
엘리트는 HP가 Boss와 다르므로 건드리지 않는다. 레이드별 추가 HP 버프(`EnemyExtraStats`)와 층 `StatChange`는 적용하지
않았다. 기대 피해량은 배율의 가중 산술평균이다. 약한 상성이 HP 대부분을 덮는 경우 시간 기준 평균(조화평균)은 이보다
낮게 나온다.

## 도구와 출력

`cargo run -p baocr-wikidb --example weight_report -- <wiki.db>`는 스테이지와 레이드 난이도 목록에 대해 머릿수 기준
구성과 HP 가중 구성을 나란히 출력하고, 각각이 공격 타입별 샘플 스트라이커 하나의 적합 등급(fit tier)을 어떻게
바꾸는지 보여 준다. 방어 타입과 지형은 같게 두므로 구성만 다르다. `x1.25→x2.00`은 머릿수 기준에서 HP 기준으로
넘어갈 때의 기대 피해 배율이고, `*`는 등급이 바뀐 곳이다. 여기서 "heads"는 모델이 세는 적만 센다. 그래서 드럼통 게는
공유 HP 바의 몸체 여섯이 이미 빠져 있다(기존 규칙은 이를 세어 특수 78 / 경 22였다).

```
wiki.db  (* = the tier changes with the weighting)

stage 30501 현상수배 - 고저택 (Lv.10)
  armor  heads: 중장갑 75% / 경장갑 25%
  armor  HP   : 경장갑 100% / 중장갑 0%
  attack heads: 일반 100%
  attack HP   : 일반 100%
  fit (heads→HP): Explosion x1.25→x2.00 보통 | Pierce x1.62→x0.50 강적합→부적합 * | Mystic x0.62→x1.00 부적합→보통 * | Sonic x0.62→x1.00 부적합

stage 30401 현상수배 - 파괴된 도로 (Lv.10)
  armor  heads: 중장갑 60% / 특수장갑 20% / 경장갑 20%
  armor  HP   : 중장갑 97% / 특수장갑 3% / 경장갑 0%
  attack heads: 일반 80% / 신비 20%
  attack HP   : 일반 97% / 신비 3%
  fit (heads→HP): Explosion x1.10→x0.99 보통 | Pierce x1.50→x1.97 보통 | Mystic x0.90→x0.55 부적합 | Sonic x0.80→x0.53 부적합

stage 1031401 KV 하이웨이 35번 도로 (Lv.20)
  armor  heads: 경장갑 60% / 중장갑 40%
  armor  HP   : 중장갑 80% / 경장갑 20%
  attack heads: 일반 80% / 폭발 20%
  attack HP   : 폭발 70% / 일반 30%
  fit (heads→HP): Explosion x1.60→x1.20 강적합→보통 * | Pierce x1.10→x1.70 부적합→보통 * | Mystic x0.80→x0.60 부적합 | Sonic x0.80→x0.60 부적합

stage 1141102 (구) 사립 히아신스 학원 (Lv.67)
  armor  heads: 경장갑 52% / 특수장갑 48%
  armor  HP   : 경장갑 83% / 특수장갑 17%
  attack heads: 일반 100%
  attack HP   : 일반 100%
  fit (heads→HP): Explosion x1.29→x1.75 보통 | Pierce x0.74→x0.58 부적합 | Mystic x1.48→x1.17 보통 | Sonic x1.24→x1.08 보통→부적합 *

stage 60304 학원교류회 - 밀레니엄 (Lv.95)
  armor  heads: 경장갑 40% / 중장갑 40% / 특수장갑 20%
  armor  HP   : 특수장갑 55% / 경장갑 24% / 중장갑 21%
  attack heads: 관통 70% / 폭발 20% / 신비 10%
  attack HP   : 관통 73% / 신비 19% / 폭발 8%
  fit (heads→HP): Explosion x1.30→x0.96 보통 | Pierce x1.20→x1.09 보통 | Mystic x1.00→x1.45 부적합→보통 * | Sonic x0.90→x1.17 부적합

raid Raid 14 드럼통 게 (Normal) (Lv.17)
  armor  heads: 특수장갑 50% / 경장갑 50%
  armor  HP   : 특수장갑 64% / 경장갑 36%
  attack heads: 일반 100%
  attack HP   : 일반 100%
  fit (heads→HP): Explosion x1.25→x1.04 적합→보통 * | Pierce x0.75→x0.82 부적합 | Mystic x1.50→x1.64 강적합 | Sonic x1.25→x1.32 적합

raid Raid 14 드럼통 게 (Insane) (Lv.80)
  armor  heads: 특수장갑 50% / 경장갑 50%
  armor  HP   : 특수장갑 68% / 경장갑 32%
  attack heads: 관통 100%
  attack HP   : 관통 100%
  fit (heads→HP): Explosion x1.25→x0.99 적합→보통 * | Pierce x0.75→x0.84 부적합 | Mystic x1.50→x1.68 적합 | Sonic x1.25→x1.34 보통

raid Raid 1 비나 (Normal) (Lv.17)
  armor  heads: 중장갑 100%
  armor  HP   : 중장갑 100%
  attack heads: 일반 100%
  attack HP   : 일반 100%
  fit (heads→HP): Explosion x1.00→x1.00 보통 | Pierce x2.00→x2.00 적합 | Mystic x0.50→x0.50 부적합 | Sonic x0.50→x0.50 부적합

raid Raid 1 비나 (Insane) (Lv.80)
  armor  heads: 중장갑 100%
  armor  HP   : 중장갑 100%
  attack heads: 관통 100%
  attack HP   : 관통 100%
  fit (heads→HP): Explosion x1.00→x1.00 보통 | Pierce x2.00→x2.00 보통 | Mystic x0.50→x0.50 부적합 | Sonic x0.50→x0.50 부적합

raid Raid 4 예로니무스 (Insane) (Lv.80)
  armor  heads: 경장갑 100%
  armor  HP   : 경장갑 100%
  attack heads: 폭발 100%
  attack HP   : 폭발 100%
  fit (heads→HP): Explosion x2.00→x2.00 보통 | Pierce x0.50→x0.50 부적합 | Mystic x1.00→x1.00 보통 | Sonic x1.00→x1.00 보통

raid Raid 12 게부라 (Normal) (Lv.17)
  armor  heads: 경장갑 50% / 일반장갑 50%
  armor  HP   : 경장갑 96% / 일반장갑 4%
  attack heads: 일반 100%
  attack HP   : 일반 100%
  fit (heads→HP): Explosion x1.50→x1.96 적합 | Pierce x0.75→x0.52 부적합 | Mystic x1.00→x1.00 부적합 | Sonic x1.00→x1.00 보통

raid Raid 5 KAITEN FX Mk.0 (Lunatic) (Lv.90)
  armor  heads: 경장갑 100%
  armor  HP   : 경장갑 100%
  attack heads: 관통 100%
  attack HP   : 관통 100%
  fit (heads→HP): Explosion x2.00→x2.00 강적합 | Pierce x0.50→x0.50 부적합 | Mystic x1.00→x1.00 부적합 | Sonic x1.00→x1.00 부적합
```

가중에 따라 결론이 바뀌는 곳은 다음과 같다. 현상수배 고저택(ChaserE)은 머릿수로는 관통이 강적합이고 HP로는 폭발이
강적합이다. KV 하이웨이는 중장갑 챔피언이 경장갑 졸개보다 비중이 크다. 드럼통 게는 머릿수로는 폭발이 적합이지만
경장갑 부위를 반영하면 보통이 된다. 바뀌지 않는 곳은 보스 하나짜리 레이드다.
실제 게임에서 확인할 것은 두 가지다. 드럼통 게의 2페이즈에서 편성이 경장갑 부위(선체, 강철 집게)에 시간을 쓰는지,
현상수배의 관문 졸개가 모델의 가정만큼 빨리 죽는지. HP 가중 비중은 시간이 아니다.
