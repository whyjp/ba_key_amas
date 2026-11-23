# How much each enemy armor and attack type counts

Problem: a mission or raid has enemies of several armor types and attack types, and they do not matter equally.
Counting heads treats a 1-HP minion and a boss alike. 드럼통 게 shows 특수장갑 and 경장갑 on its screen while the
lead enemy in the DB is 특수장갑 only. This note says what the sources support, what the data offers, which model the
app uses and why. Evidence is cited; inference is labelled as such.

Labels: **DATA** read raw from SchaleDB JSON or JS by us. **GUIDE** a community or wiki page as summarised by a
fetch (not verbatim). **INFERENCE** ours.

## Static and dynamic data

Dungeon and student judgments come from the static DB. The screen only says which encounter the player picked.

| Datum | Static / dynamic | Source | Consumed by |
| --- | --- | --- | --- |
| Which screen, mode, mission or boss name and number, difficulty | dynamic | screen readers (`extract`) | lookup keys in `assist::observe`, `raid_lookup::lookup` |
| Students in the squad (names, slots), selection | dynamic | screen readers | `Wiki::match_student`, formation widgets |
| Enemy armor and attack type mix (shares) | static | `v_stage_*_share`, `v_raid_*_share` (from `*_enemy_weight`) | `MissionBrief.armor/attack`, `formation::evaluate`, matchup tables |
| Enemy roster, level, HP, weight | static | `formation_enemy_weight`, `raid_difficulty_enemy_weight` | the share views, `examples/weight_report.rs` |
| Terrain of a stage, recommended level | static | `stage.terrain`, `stage.level`, `raid_season.terrain`, `raid_difficulty.level` | `MissionBrief` |
| Student attack, armor, terrain adaptation, role | static | `student` | `StudentBrief`, `fit_tier` |
| Type effectiveness | static | `type_effectiveness` | `TypeTable` |
| Armor and attack words on a raid screen | dynamic, cross-check only | screen readers (`RaidScreenWords`) | `raid_lookup::cross_check`, a diagnostic row; never a value |

## What the sources say

1. No opened source gives a rule for mixed boss and minion groups in normal stages (GUIDE game8
   `https://game8.jp/blue-archive/656430`: check the content's defense type and build around the effective attack
   type; nothing about weighting). Not found: any head-count, HP or wave rule.
2. Total assault is scored on time (GUIDE `https://bluearchive.wiki/wiki/Total_Assault`: "based on total time spent in
   combat"). The boss is one large HP pool, so its armor sets the multiplier on almost all score-relevant HP
   (INFERENCE).
3. Parts and summons are not uniform. GUIDE (raid strategy doc,
   `https://docs.google.com/document/d/1bvYGwIh-9o-2JQ4Q2MQXtsL0peiCPrRuw0l3HQmTrZ0`): damage to Gregorius' choir "will
   be reflected onto Greg"; damage to the Hovercraft MGS "will be reflected onto the Hovercrafts HP"; Geburah's
   radiator "cannot be damaged by your team". A part can feed the boss bar, be an obstacle or be ignorable, per boss.
4. Drum Crab (DATA `https://schaledb.com/data/kr/raids.min.json`, `enemies.min.json`): the raid record has
   `ArmorType: Unarmed` and `SubArmorType: LightArmor` (the only raid with a sub armor); the live raid card renders
   both (DATA `TimeAttackCard-*.js` from `https://schaledb.com/assets/`). The Boss unit alone has `PhaseChange`
   (HP under 200000 of 400000 on Normal) and its dev name is `EN0022_TotalHP_...`. The "(1 페이즈)", "(2 페이즈)",
   배딱지 and 부패한 드럼통 entries list the same 400000 HP as the Boss. Parts have their own HP: claw 3000 (특수),
   hull 25000 (경장갑), steel pincer 200000 (경장갑). Skill text: "배딱지, 집게발, 선체로 구성되며, 각각 개별적인 방어 타입".
   GUIDE (ruliweb `https://bbs.ruliweb.com/etcs/board/185438/read/1398296`, `https://bluearchive.wiki/wiki/Drumbarka`):
   phase 1 body 특수 (default target), hull 경장갑 destructible; phase 2 steel pincer 경장갑, EX only. So both armors
   are really fought.
5. Phases: phase 1 holds about 55 % of the HP (PhaseChange thresholds, DATA); phase 1 takes about 2 min and phase 2
   about 1 min 30 s (GUIDE ruliweb). No source quantifies time per part.
6. Attack type: Normal attacks are 1.0 against every student armor (GUIDE game8); Insane and above change the boss
   attack type (DATA `BulletTypeInsane`; Binah and Drum Crab Pierce, Hod Mystic; GUIDE bluearchive.wiki Binah, Hod pages).
7. Stats: there is no rank multiplier. Each enemy has `MaxHP1` and `MaxHP100`; the stat at level L is
   `ceil(round(s1 + (s100 - s1) * round4((L-1)/99)) * grade multiplier)`, grade multiplier `1 + sum(HP steps[0..grade]) / 10000`
   with default steps `[0, 500, 700, 900, 1400]`; the level of a stage enemy is `Level<Rank>` else `LevelMinion`, its
   grade `Grade<Rank>` else `GradeMinion` (DATA, live bundle `https://schaledb.com/assets/index-c2394870.js`, `RaidView-*.js`).
   Raid levels default to `[17,25,35,50,70,80,90,90]` by difficulty (same bundle). Raid HP is set per difficulty as
   flat values (`MaxHP1 == MaxHP100`).

Not settled: whether Drum Crab part damage also reduces the main bar; the hull's targetability (EX only or always);
the meaning of `ArmorTypeRestriction`; any time-per-part data; any community rule for mixed normal stages.

## What the data offers

Stage formations are map units fought one after another (`stage_formation.ordinal`, the wave). Enemy `Rank` is Minion,
Elite, Champion, Boss, Summoned, DestructibleProjectile, Hallucination, Student. Per-enemy HP, attack power and level
are in the source; `TacticRole` does not exist on enemies. Stage records carry an `ArmorTypes` list that is a set, not
a weighting. 340 stages, 1325 formations: one enemy holds more than 90 % of a stage's HP in 13 stages; bounty ChaserE
has a 70,012,000 HP champion (a sentinel-sized value) next to 77,000 HP and minions of about 200 HP.

## Three models on real rows

A head count (the old rule). B HP weighted (each enemy counts its HP at the encounter's level and grade, summons,
escorted transports and shootable projectiles weigh 0). C = B plus one structural rule for raids: an entry that is not a
Boss and has exactly a Boss's HP is that Boss's own body and weighs 0 (phases, skins and the shared bar). No numeric
role multiplier: no source supports one.

| Case (DB rows) | A heads | B HP | C chosen |
| --- | --- | --- | --- |
| 드럼통 게 Normal | 특수 78 / 경 22 | 특수 91 / 경 9 | 특수 64 / 경 36 |
| 드럼통 게 Insane | 특수 78 / 경 22 | 특수 93 / 경 7 | 특수 68 / 경 32 |
| 비나 (single boss, any difficulty) | 중 100 | 중 100 | 중 100 (attack 일반, Insane 관통) |
| KV 하이웨이 35번 도로 (Champion 중장갑, 2 elites, 2 light minions) | 중 40 / 경 60 | 중 80 / 경 20 | same as B (stages have no mirrors) |
| (구) 사립 히아신스 학원: wave 1 and 2 elites and minions 특수, wave 3 elites 경, wave 4 champion 경 | 특수 48 / 경 52 | 특수 17 / 경 83 | same as B |
| 현상수배 ChaserE Lv.10 (2 light champions, 6 heavy minions) | 경 25 / 중 75 | 경 100 / 중 0 | same as B |

Reading: A over-weights swarms (ChaserE says the enemy is mostly heavy; the light champions are the fight). B follows
the boss and the boss wave, but on Drum Crab it counts the shared 400k bar six times (Boss, two bodies, 배딱지 twice,
부패한 드럼통) and so hides the light parts, which the guides say are fought. C counts the bar once and keeps the parts
with their own HP, so the DB alone gives both armors, the way the screen shows them. C is chosen.

Limits, said plainly. HP share is not time share: area attacks kill swarms together, which B and C ignore, so ChaserE's
0 % heavy is a lower bound. The 70M sentinel in ChaserE does not change that stage's armor (both champions are light).
The mirror rule is an inference from the Drum Crab data and the `TotalHP` dev name; it hits the Drum Crab and the
Kaitenger elites are untouched (their HP differs from the Boss). Per-raid extra HP buffs (`EnemyExtraStats`) and floor
`StatChange` are not applied. The expected-damage figure is a weighted arithmetic mean of the multipliers; the
time-based mean (harmonic) would sit lower when a weak match covers much HP.
