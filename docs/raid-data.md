# Raid data (총력전, 종합전술시험, 월드 레이드, ...)

What the wiki DB knows about raid-like encounters, where it comes from, what it cannot know, and what a screen
reader has to hand it. Schema: `wikidb/schema.sql` (version 4). Code: `rust/crates/wikidb/src/schaledb.rs`
(collector) and `rust/crates/wikidb/src/raid.rs` (lookup).

## Model

The source (SchaleDB `raids.json`) keeps five kinds of encounter, each with its own shape. The collector parses them
once, at the boundary, into the same four tables. Nothing downstream reads the source JSON.

| Table | One row is | Notes |
| --- | --- | --- |
| `raid` | one encounter, key `(kind, id)` | `kind` is `Raid` (총력전), `TimeAttack` (종합전술시험), `WorldRaid`, `MultiFloorRaid`, `InteractiveWorldRaid`. Ids repeat across kinds. `raw` keeps the source record. |
| `raid_difficulty` | one difficulty of an encounter, key `(raid_kind, raid_id, idx)` | `idx` is the position in the source arrays. `released_global` is true when the raid is released on Global and `idx` is within the Global `MaxDifficulty`. |
| `raid_difficulty_enemy` | one enemy of a difficulty, in source order | Links to `enemy`. Summons, projectiles and escorted transports are here too: filter on `enemy.rank`. |
| `raid_season` | one Global season, key `(mode, season_id)` | `mode` is `Raid` (총력전) or `Eliminate` (대결전). The season's `terrain` is the terrain the raid is played on. `start_at` and `end_at` are unix seconds. |

`v_raid_difficulty` is a difficulty with its lead enemy (name, attack type, armor type).

### Columns that need a sentence

- `raid.name_ko` is the source's name. It is NULL for every `TimeAttack`: the source names none.
- `raid.display_ko` is what to show and to match against. It is `name_ko`, or for a time attack a name derived
  from the record: test, terrain and enemy armor, for example `사격 시험 · 야외 · 경장갑`. The derived name is
  not a string the game is known to show; a screen reader that sees the real wording should compare it with this.
- `raid.terrain` is the list of terrains the raid can be played on. It is a candidate list, not the answer: the
  answer is the season's terrain (`raid_season.terrain`).
- There is no attack type or armor type on `raid` any more. They are per enemy and change with the difficulty. The
  old single `raid.bullet_type` and `raid.armor_type` held the Normal difficulty's value only and were wrong for
  Insane and above (Binah is Pierce from Insane, the column said Normal).
- `raid_difficulty.label` is the source's own name: `Normal`, `Hard`, `VeryHard`, `HardCore`, `Extreme`, `Insane`,
  `Torment`, `Lunatic` for `Raid` (by position, see below), `DifficultyName` for world raids (`A`..`D` for interactive
  ones; one world raid starts at `HardCore`), none for time attacks and floor raids.
- `raid_difficulty.level` is the enemy level when the source gives one (time attacks, world raids). Time attacks
  have four levels (25, 40, 65, 78) and no names.
- `raid_difficulty.floor_start` is the first floor of a `MultiFloorRaid` tier.
- `raid_difficulty.boss_enemy_id` is the first enemy ranked Boss, else the first Champion, else NULL (a time attack
  of the defense or destruction kind has neither).

### Per-kind shapes the collector reads

| Kind | Difficulties come from | Label |
| --- | --- | --- |
| `Raid` | `EnemyList[idx]`, `BattleDuration[idx]` | position in `Normal, Hard, VeryHard, HardCore, Extreme, Insane, Torment, Lunatic` |
| `TimeAttack` | `Formations[idx].EnemyList`, `Level[idx]`, `BattleDuration[idx]` | none |
| `WorldRaid`, `InteractiveWorldRaid` | `EnemyList[idx]`, `Level[idx]`, `BattleDuration[idx]` | `DifficultyName[idx]` |
| `MultiFloorRaid` | `EnemyList[tier]`, `DifficultyStartFloor[tier]`, one `BattleDuration` | none |

The `Raid` label order is an inference: SchaleDB's `RaidDifficulty` localization lists these names, and the attack
type of the enemies switches at position 5, where the record's `BulletTypeInsane` applies. A test pins it.

## What the data says about attack types

Attack and armor type are properties of each enemy in a difficulty, not of the raid. A boss changes attack type
with the difficulty: Binah is Normal on positions 0 to 4 and Pierce on 5 to 7, Hieronymus Normal then Explosion.
Minions and summons can differ from the boss (Hieronymus' relics stay Normal). The mission shape takes armor and attack types as shares of the enemy HP (`v_raid_armor_share`,
`v_raid_attack_share`), see `docs/enemy-weighting.md`; ranks `Summoned`, `Student` and `DestructibleProjectile`
weigh nothing.

## Lookup API (`baocr_wikidb`, module `raid`)

All functions are on `Wiki`, pure over the DB, and take the clock as an argument.

- `raids_by_name(name) -> Vec<RaidCandidate>`. Global-released raids whose name equals `name` (spaces and punctuation
  ignored); when none does, those whose name starts with it. Several candidates are returned, never the first:
  "비나" is a `Raid` and an `InteractiveWorldRaid`, "게부라" likewise, a name can also be a time attack. Candidates are
  ordered by kind then id. `exact` tells whole-name from prefix.
- `raid_difficulties(kind, id) -> Vec<RaidDifficulty>`. All of them, closed ones flagged by `released_global`.
- `raid_seasons(raid_id)` and `current_or_nearest_season(raid_id, now) -> Option<SeasonPick>`. The season running at
  `now`; else the nearest one in time, with `relation` `Active`, `Upcoming` or `Past`; a tie goes to the one that
  ended; several running at once, the latest start. Only total assaults have seasons.
- `raid_mission(kind, id, idx, season) -> Option<MissionBrief>`. The existing mission shape, so
  `formation::evaluate` is unchanged. `stage_id` is the raid id, `difficulty` the difficulty `idx`, `terrain` the
  season's (else the raid's only terrain, else unknown), `recommended_level` the enemy level.
  The shape needed no extension: per-armor and per-attack counts carry what a raid has.

## Known gaps

- Multi-party rules. A total assault uses several squads (a striker party per attempt, a boss that needs the
  squads to differ); none of that is in SchaleDB. The verdict computed from `raid_mission` judges one squad.
- 대결전 (Eliminate) armor choice. `raid_season.open_difficulty` (mode `Eliminate`) lists, per armor type, the
  highest open difficulty. The enemy data holds one armor per boss; how the three armor runs map onto the enemy
  records is not modelled. `raid_mission` for an Eliminate season uses the enemy records as they are.
- Time attacks have no source name and the game's wording is unknown. Several time attacks share the same display
  name (same test, terrain, armor). Those duplicates have identical enemies and differ only in battle duration and
  Global release; when `raids_by_name` returns several candidates of one display name, their missions are equal.
- Time-attack rules (`TimeAttackRules`), rewards, skills and HP/attack stats are only in `raw`.
- Floor raids: the last tier's end floor is not modelled (`DifficultyStartFloor` has one more boundary than there are
  tiers).
- Season and difficulty availability reflect the SchaleDB snapshot; a season the game has added since is missing
  until the DB is rebuilt.

## What the screen-reading step must provide

To use the lookup the reader must supply:

1. The boss name text as the game shows it (OCR), which goes to `raids_by_name`. For a time attack the test, terrain
   and armor wording instead.
2. Which kind of screen it is (총력전, 종합전술시험, 월드 레이드, ...), to settle candidates that share a name.
3. The difficulty, as the game's difficulty name or number. For a total assault it maps to `idx` through
   `RaidDifficulty::label` (`Normal` .. `Lunatic`). Without it, the attack types can only be given per difficulty.
4. The season (the number the game shows, or simply the current time) for the terrain: `now` goes to
   `current_or_nearest_season`. A `Past` or `Upcoming` relation means the screen may be of another season; if the
   screen shows its terrain, prefer that over the season's.
5. For 대결전, the armor type of the run being shown.

Rebuild the DB (`cargo run -p baocr-wikidb --release`) whenever `Wiki::open` reports a schema version mismatch.
`cargo run -p baocr-wikidb --example raid_report -- <wiki.db>` prints the row counts and checks the invariants.

## Raid screens

What the add-on reads on screen and how it reaches the lookup above. Code: `extract/src/scene.rs` (screens),
`extract/src/raid.rs` (encounter from screen text), `assist/src/raid_lookup.rs` (screen to `MissionBrief`),
`assist/src/widgets.rs` (`RaidInfo` card, the formation verdict). All of it is read from fixtures in
`fixtures/screens/` (`*.boxes.txt` OCR boxes, PNG for some); nothing here was driven live. 4:3 frames only.

| Screen | Scene | What it yields | Evidence |
| --- | --- | --- | --- |
| 업무 (all modes) | `WorkHub` | nothing | `work-hub-full` |
| 총력전 "Battle Select" | `RaidSelect` | boss, difficulty rows Normal..Extreme | `raid-select` (png: `raid-select-hieronymus`) |
| 모의 총력전 boss list | `MockRaidBosses` | boss, armor words (a dual armor is two), the boss's two terrain tabs | `raid-mock-m00..m14` (14 bosses, m14 repeats m00) |
| 방 정보 popup | `RaidRoomInfo` | difficulty, boss, level, attack, armor | `raid-mock-room-info` |
| 편성 of 총력전, 모의 총력전, 제약해제결전 | `RaidFormation(mode)` | the mode, from the header only; no boss and no student name on screen | `raid-formation-empty`, `unrestricted-raid-formation` |
| 제약해제결전 select, 단계 bands, one stage | `UnrestrictedRaidSelect`, `Stages`, `Stage` | boss; boss, level, attack, armor (stage screen) | `unrestricted-raid-select`, `-stages`, `-stage01` |
| 종합전술시험, "다음 시즌 정보" popup | `TimeAttackSelect`, `TimeAttackNextSeason` | scene label only | `timeattack-select`, `timeattack-next-season` |
| 대결전 between seasons | `EliminateClosed` | scene label only | `eliminate-closed` |
| Warning before 제약해제결전 | `SpoilerWarning` | scene label only | `spoiler-warning` |

Scenes are told by the header text plus words only that screen has (e.g. "Battle Select", "보스 선택", the popup titles
"방 정보" and "다음 시즌 정보"), never by position alone. The mode of a formation comes from its header
(`총력전 편성`, `모의 총력전 편성`, `제약해제결전 편성`), because a mock screen also shows "총력전" as a side button.

### From screen text to the card

1. `RaidTracker` remembers the encounter (`RaidRef`: mode, boss text, difficulty position, armor words, attack word, level,
   terrain tabs) from the list, the popup or the stage screen, so the formation, which names none of it, knows it. A mission
   screen forgets the raid and a raid screen forgets the mission. The mode of a formation must match the remembered encounter.
2. `raid_lookup::lookup` finds the raid by boss name among the raids of the mode's kind (총력전 and 모의 총력전: `Raid`,
   제약해제결전: `MultiFloorRaid`), decodes the armor and attack words through the `label` table (`Wiki::decode_labels`, per
   kind: 일반 is an attack type, 일반장갑 an armor type; a word the table lacks is reported, not guessed), maps the
   difficulty word to `idx` by its position (Normal 0 .. Lunatic 7; the game writes `Hardcore`, the DB `HardCore`), takes the
   terrain from the 총력전 season (only a `Raid` mode season; 대결전 seasons are ignored), and builds the brief with
   `raid_mission`.
3. The screen is the truth for what the player sees. Where it names the boss's armor, attack type or level, those replace the
   wiki's (`Wiki::with_screen_types` swaps the lead enemy's types in the tally and recomputes the best attack types), and a
   difference is shown on the card ("화면 ... / 위키 ...").
4. Nothing is picked silently. Several raids of one name stay `Ambiguous` unless the on-screen armor tells them apart (the two
   세트의 분노 differ by armor: 경장갑 is id 1000001). A screen that names no difficulty (the boss lists) is not given Normal:
   the card says "선택 전", lists the attack type per difficulty range, and the formation card says which difficulty its verdict used.

### Findings in the data

- The armor read from the 14 mock bosses equals the DB armor of the Normal lead enemy for 13. 드럼통게 shows
  `특수장갑/경장갑` and the DB lead enemy is `Unarmed` only; its other enemies (파손된 선체, 강철 집게) are `LightArmor`, so
  the dual armor is the boss plus its parts, and the screen is consistent with the whole enemy list. Checked on the real DB:
  `BAOCR_WIKI_DB=<wiki.db> cargo test -p baocr-assist real_database -- --nocapture`.
- `raid_seasons` returns 대결전 seasons of the same raid id too; use only `SeasonMode::Raid` for a total assault.

### Formation card

The formation widget is the existing one over the raid's brief. 총력전 and 모의 총력전 use up to four parties (1부대 .. 4부대); the
card says so and that the verdict judges the one party last read. The formation screen shows no student names (the
제약해제결전 one shows portraits and levels), so the squad is read through the 빠른 편성 popup as for missions. Which party
tab is being edited is not read. No multi-party optimisation.

### Not handled

- 대결전 while a season runs (no fixture; any 대결전 header that is not the closed one is `EliminateSelect`, untested), and its
  armor runs. 전술 대회 (PvP). 종합전술시험 and 대결전 have no card, only the scene label.
- The Insane, Torment and Lunatic rows of the 총력전 list below the fold (the list scrolls); the 모의 총력전 list shows them
  as far as they are on screen. The row chosen on the 제약해제결전 단계 bands (a highlight, not text) and the terrain of
  its stage (an icon). Which of a boss's two terrain tabs is highlighted (a colour).
- The real 총력전 entry popup and 총력전 formation (not recorded; `총력전 편성` is classified by its header by inference).
- 빠른 편성 opened over a raid formation was not recorded. Its student grid is assumed to be the mission one (icons are drawn);
  the strip of slots below it is not known for raids, so the slot icons are off there.
- Frames that are not 4:3.
