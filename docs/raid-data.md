# Raid data (총력전, 종합전술시험, 월드 레이드, ...)

What the wiki DB knows about raid-like encounters, where it comes from, what it cannot know, and what a screen
reader has to hand it. Schema: `wikidb/schema.sql` (version 3). Code: `rust/crates/wikidb/src/schaledb.rs`
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
Minions and summons can differ from the boss (Hieronymus' relics stay Normal). The mission shape counts enemies the
way stage missions are counted, excluding ranks `Summoned`, `Student` (escorted transports) and
`DestructibleProjectile`.

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
