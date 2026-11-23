-- Blue Archive reference database ("wiki DB"). Global server (the Korean client is Global).
-- Reference data only, rebuilt from sources by the collectors. The user's own roster stays
-- in the scan output (index.json) and joins to this DB by student.id.
-- Local assist use only: the DB file is not distributed (see source.note).
-- Version 3. Bump meta.schema_version on every incompatible change.

PRAGMA foreign_keys = ON;

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- Where a row came from. Every entity row names its source, so a collector can replace
-- exactly its own rows and a reader can show provenance.
CREATE TABLE source (
    id          INTEGER PRIMARY KEY,
    name        TEXT NOT NULL UNIQUE,       -- 'schaledb' | 'arisu-archive'
    url         TEXT NOT NULL,
    fetched_at  TEXT NOT NULL,              -- ISO 8601 UTC
    version     TEXT,                       -- upstream data version / commit, when it has one
    note        TEXT                        -- licence, attribution and use conditions
);

-- Korean display names for the closed code sets.
CREATE TABLE label (
    kind TEXT NOT NULL,                     -- 'BulletType' | 'ArmorType' | 'Terrain' | 'StageType' | 'EnemyRank' | 'TimeAttackStage' | ...
    code TEXT NOT NULL,
    ko   TEXT NOT NULL,
    PRIMARY KEY (kind, code)
);

-- Damage multiplier of an attack (bullet) type against an armor type, in basis points (10000 = 100 %).
CREATE TABLE type_effectiveness (
    bullet_type   TEXT NOT NULL,
    armor_type    TEXT NOT NULL,
    multiplier_bp INTEGER NOT NULL,
    PRIMARY KEY (bullet_type, armor_type)
);

CREATE TABLE student (
    id              INTEGER PRIMARY KEY,    -- game student id, same as the scan's student id
    path_name       TEXT NOT NULL,
    dev_name        TEXT NOT NULL,
    name_ko         TEXT NOT NULL,
    school          TEXT,
    club            TEXT,
    star_grade      INTEGER NOT NULL,       -- initial stars
    squad_type      TEXT NOT NULL,          -- 'Main' (STRIKER) | 'Support' (SPECIAL)
    tactic_role     TEXT,
    position        TEXT,
    bullet_type     TEXT NOT NULL,          -- attack type
    armor_type      TEXT NOT NULL,
    street_adapt    INTEGER,                -- terrain adaptation grade (higher is better)
    outdoor_adapt   INTEGER,
    indoor_adapt    INTEGER,
    weapon_type     TEXT,
    released_global INTEGER NOT NULL CHECK (released_global IN (0, 1)),
    source_id       INTEGER NOT NULL REFERENCES source(id),
    raw             TEXT NOT NULL CHECK (json_valid(raw))   -- the source record, for fields not modelled yet
);

CREATE TABLE enemy (
    id          INTEGER PRIMARY KEY,
    dev_name    TEXT,
    name_ko     TEXT NOT NULL,
    rank        TEXT NOT NULL,              -- Minion | Elite | Champion | Boss | Summoned | ...
    squad_type  TEXT,
    bullet_type TEXT,
    armor_type  TEXT,
    size        TEXT,
    source_id   INTEGER NOT NULL REFERENCES source(id),
    raw         TEXT NOT NULL CHECK (json_valid(raw))     -- stats and skills stay here until a reader needs them
);

-- Campaign missions, bounties (현상수배), 특별의뢰, 학원교류회.
CREATE TABLE stage (
    id                  INTEGER PRIMARY KEY,
    category            TEXT NOT NULL,      -- Campaign | Bounty | WeekDungeon | SchoolDungeon
    type                TEXT,               -- ChaserA-E, FindGift, Blood, SchoolA-C; NULL for Campaign
    area                INTEGER,            -- campaign area (1-x)
    stage_no            INTEGER,
    difficulty          INTEGER,            -- campaign: 0 normal, 1 hard
    name_ko             TEXT,
    terrain             TEXT,               -- Street | Outdoor | Indoor
    level               INTEGER,
    entry_cost          TEXT CHECK (entry_cost IS NULL OR json_valid(entry_cost)),
    star_condition      TEXT CHECK (star_condition IS NULL OR json_valid(star_condition)),
    challenge_condition TEXT CHECK (challenge_condition IS NULL OR json_valid(challenge_condition)),
    source_id           INTEGER NOT NULL REFERENCES source(id)
);
CREATE INDEX stage_lookup ON stage (category, area, stage_no, difficulty);
CREATE INDEX stage_name ON stage (name_ko);

CREATE TABLE stage_formation (
    id           INTEGER PRIMARY KEY,       -- the game's formation id
    stage_id     INTEGER NOT NULL REFERENCES stage(id) ON DELETE CASCADE,
    ordinal      INTEGER NOT NULL,
    unit_grade   TEXT,                      -- Grade1 | Boss | ...
    level_minion INTEGER,
    grade_minion INTEGER
);
CREATE INDEX stage_formation_stage ON stage_formation (stage_id);

CREATE TABLE formation_enemy (
    formation_id INTEGER NOT NULL REFERENCES stage_formation(id) ON DELETE CASCADE,
    ordinal      INTEGER NOT NULL,
    enemy_id     INTEGER NOT NULL REFERENCES enemy(id),
    PRIMARY KEY (formation_id, ordinal)
);

-- Total assault (총력전), 종합전술시험, world raids, ...: a boss encounter. One row per
-- encounter; what the enemies are (attack type, armor type, level, time limit) differs per
-- difficulty and lives in raid_difficulty. `raw` keeps the whole source record.
CREATE TABLE raid (
    id              INTEGER NOT NULL,
    kind            TEXT NOT NULL,          -- Raid | TimeAttack | WorldRaid | MultiFloorRaid | InteractiveWorldRaid
    path_name       TEXT,
    name_ko         TEXT,                   -- the source's name; NULL for TimeAttack (the source names none)
    display_ko      TEXT NOT NULL,          -- what to show and match: name_ko, else derived from the record (TimeAttack: "사격 시험 · 야외 · 경장갑")
    dungeon_type    TEXT,                   -- TimeAttack only: Shooting | Defense | Destruction | Escort
    terrain         TEXT CHECK (terrain IS NULL OR json_valid(terrain)),   -- JSON array of the terrains the encounter can be played on; the real one is the season's
    released_global INTEGER CHECK (released_global IN (0, 1)),
    source_id       INTEGER NOT NULL REFERENCES source(id),
    raw             TEXT NOT NULL CHECK (json_valid(raw)),
    PRIMARY KEY (kind, id)
);
CREATE INDEX raid_display ON raid (display_ko);

-- One difficulty of a raid, parsed from the source's per-difficulty arrays. `idx` is the
-- position in those arrays. Attack type and armor type are per enemy (enemy table): a boss
-- changes attack type with the difficulty (Binah: Normal up to Extreme, Pierce from Insane).
CREATE TABLE raid_difficulty (
    raid_kind       TEXT NOT NULL,
    raid_id         INTEGER NOT NULL,
    idx             INTEGER NOT NULL,
    label           TEXT,                   -- the source's name: Normal .. Lunatic, A .. D; NULL for TimeAttack and MultiFloorRaid
    level           INTEGER,                -- enemy level when the source gives one
    floor_start     INTEGER,                -- MultiFloorRaid: first floor of this tier
    battle_duration INTEGER,                -- seconds
    boss_enemy_id   INTEGER REFERENCES enemy(id),   -- first enemy ranked Boss, else Champion, else NULL
    released_global INTEGER NOT NULL CHECK (released_global IN (0, 1)),   -- raid released and idx within the Global MaxDifficulty
    PRIMARY KEY (raid_kind, raid_id, idx),
    FOREIGN KEY (raid_kind, raid_id) REFERENCES raid(kind, id) ON DELETE CASCADE
);

-- The enemies of a raid difficulty, in the source's order (summons and projectiles included:
-- filter on enemy.rank).
CREATE TABLE raid_difficulty_enemy (
    raid_kind TEXT NOT NULL,
    raid_id   INTEGER NOT NULL,
    idx       INTEGER NOT NULL,
    ordinal   INTEGER NOT NULL,
    enemy_id  INTEGER NOT NULL REFERENCES enemy(id),
    PRIMARY KEY (raid_kind, raid_id, idx, ordinal),
    FOREIGN KEY (raid_kind, raid_id, idx) REFERENCES raid_difficulty(raid_kind, raid_id, idx) ON DELETE CASCADE
);
CREATE INDEX raid_difficulty_enemy_enemy ON raid_difficulty_enemy (enemy_id);

-- Global seasons of 총력전 (mode 'Raid') and 대결전 (mode 'Eliminate'). The terrain of a
-- season is the one the raid is played on; start_at and end_at are unix seconds.
CREATE TABLE raid_season (
    mode            TEXT NOT NULL CHECK (mode IN ('Raid', 'Eliminate')),
    season_id       INTEGER NOT NULL,
    season_display  TEXT,                   -- the number the game shows ("BETA", "88")
    raid_kind       TEXT NOT NULL DEFAULT 'Raid' CHECK (raid_kind = 'Raid'),
    raid_id         INTEGER NOT NULL,
    terrain         TEXT NOT NULL,
    start_at        INTEGER NOT NULL,
    end_at          INTEGER NOT NULL,
    open_difficulty TEXT CHECK (open_difficulty IS NULL OR json_valid(open_difficulty)),   -- Eliminate only: highest open difficulty per armor type
    PRIMARY KEY (mode, season_id),
    FOREIGN KEY (raid_kind, raid_id) REFERENCES raid(kind, id)
);
CREATE INDEX raid_season_raid ON raid_season (raid_id, start_at);

-- Cafe furniture, decorations and interiors.
CREATE TABLE furniture (
    id                 INTEGER PRIMARY KEY,
    name_ko            TEXT NOT NULL,       -- not unique: shown by name in game, so readers must handle same-name items
    category           TEXT NOT NULL,       -- Interiors | Furnitures | Decorations | ...
    sub_category       TEXT,
    rarity             TEXT,
    size_w             INTEGER,
    size_h             INTEGER,
    comfort            INTEGER,             -- comfort bonus
    set_group_id       INTEGER,
    released_global    INTEGER NOT NULL CHECK (released_global IN (0, 1)),
    interaction_global INTEGER NOT NULL CHECK (interaction_global IN (0, 1)),   -- the source's own flag, kept to cross-check furniture_interaction
    source_id          INTEGER NOT NULL REFERENCES source(id),
    raw                TEXT NOT NULL CHECK (json_valid(raw))
);
CREATE INDEX furniture_name ON furniture (name_ko);

-- Students who interact with a furniture item in the cafe (Global server only).
CREATE TABLE furniture_interaction (
    furniture_id INTEGER NOT NULL REFERENCES furniture(id),
    student_id   INTEGER NOT NULL REFERENCES student(id),
    extra        INTEGER NOT NULL,          -- the source's second number, stored as is. SchaleDB's page reads it as the interaction kind: 3 solo, 2 group, 1 solo or group, 0 group (solo or group when another student has 2)
    PRIMARY KEY (furniture_id, student_id)
);
CREATE INDEX furniture_interaction_student ON furniture_interaction (student_id);

-- What a stage's enemies are made of: how many enemies of each armor type it holds.
CREATE VIEW v_stage_armor AS
SELECT f.stage_id AS stage_id, e.armor_type AS armor_type, COUNT(*) AS enemies
FROM stage_formation f
JOIN formation_enemy fe ON fe.formation_id = f.id
JOIN enemy e ON e.id = fe.enemy_id
WHERE e.armor_type IS NOT NULL AND e.rank <> 'Summoned'
GROUP BY f.stage_id, e.armor_type;

-- How well each student's attack type does against a stage's enemies (average multiplier,
-- weighted by enemy count, in basis points) and how well the student fits the terrain.
CREATE VIEW v_student_vs_stage AS
SELECT s.id AS stage_id,
       st.id AS student_id,
       CAST(ROUND(1.0 * SUM(a.enemies * te.multiplier_bp) / SUM(a.enemies)) AS INTEGER) AS effectiveness_bp,
       CASE s.terrain WHEN 'Street' THEN st.street_adapt
                      WHEN 'Outdoor' THEN st.outdoor_adapt
                      WHEN 'Indoor' THEN st.indoor_adapt END AS terrain_adapt
FROM stage s
JOIN v_stage_armor a ON a.stage_id = s.id
JOIN student st ON st.released_global = 1
JOIN type_effectiveness te ON te.bullet_type = st.bullet_type AND te.armor_type = a.armor_type
GROUP BY s.id, st.id;

-- A raid difficulty with its lead enemy.
CREATE VIEW v_raid_difficulty AS
SELECT d.raid_kind, d.raid_id, d.idx, d.label, d.level, d.floor_start, d.battle_duration, d.released_global,
       d.boss_enemy_id, b.name_ko AS boss_name_ko, b.bullet_type AS boss_bullet_type, b.armor_type AS boss_armor_type
FROM raid_difficulty d LEFT JOIN enemy b ON b.id = d.boss_enemy_id;

INSERT INTO meta (key, value) VALUES ('schema_version', '3');
