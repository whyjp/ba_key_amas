-- One row per user. The key (nickname#hex6) is never stored, only its HMAC.
CREATE TABLE users (
    id           INTEGER PRIMARY KEY,
    nick_norm    TEXT NOT NULL UNIQUE,          -- NFKC + lower case: the duplicate check
    nickname     TEXT NOT NULL,                 -- as the scanner read it
    key_hash     TEXT NOT NULL,                 -- hex HMAC-SHA256 of the key
    fail_count   INTEGER NOT NULL DEFAULT 0,
    locked_until INTEGER NOT NULL DEFAULT 0,    -- unix seconds
    account_json TEXT,
    progress_json TEXT,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL
);

CREATE TABLE students (
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    student_id  INTEGER,                        -- catalog id (portrait), null when the name is not in the catalog
    level       INTEGER,
    stars       INTEGER,
    ex_skill    INTEGER,
    normal_skill INTEGER,
    passive_skill INTEGER,
    sub_skill   INTEGER,
    bond        INTEGER,
    uw_stars    INTEGER,
    uw_level    INTEGER,
    role        TEXT,
    pinned      INTEGER,
    PRIMARY KEY (user_id, name)
) WITHOUT ROWID;
