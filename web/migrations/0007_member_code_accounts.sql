-- Accounts are identified by the hash of the game's member code, not by nickname#hex6.
--
-- This redesigns the users table, so it DROPS the rows of the old shape (users and students). The server has not
-- been deployed and holds only pre-release test data. Do not apply this to a database with data worth keeping.
--
--   account_hash  HMAC-SHA256(KEY_PEPPER, "account:" + account_id). Recognises the account on upload. UNIQUE.
--   public_id     first 16 hex of HMAC-SHA256(KEY_PEPPER, "public:" + account_id). What URLs and the leaderboard use. UNIQUE.
--   nickname      display text as the scanner read it. Not unique: two teachers may share a name.
--   nick_search   the folded nickname, with a plain index for prefix search among public accounts.
DROP TABLE IF EXISTS students;
DROP TABLE IF EXISTS users;

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    account_hash  TEXT NOT NULL UNIQUE,
    public_id     TEXT NOT NULL UNIQUE,
    nickname      TEXT NOT NULL,
    nick_search   TEXT NOT NULL,
    account_json  TEXT,
    progress_json TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    public        INTEGER NOT NULL DEFAULT 0,
    score         REAL NOT NULL DEFAULT 0,
    stats_json    TEXT,
    top_json      TEXT,
    score_version INTEGER NOT NULL DEFAULT 0,
    addon_version TEXT,
    protocol      INTEGER,
    chars_json    TEXT
);
CREATE INDEX users_board ON users (public, score DESC);
CREATE INDEX users_nick ON users (public, nick_search);

CREATE TABLE students (
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    student_id    INTEGER,
    level         INTEGER,
    stars         INTEGER,
    ex_skill      INTEGER,
    normal_skill  INTEGER,
    passive_skill INTEGER,
    sub_skill     INTEGER,
    bond          INTEGER,
    uw_stars      INTEGER,
    uw_level      INTEGER,
    role          TEXT,
    pinned        INTEGER,
    memorial      INTEGER,
    detail_json   TEXT,
    bg            TEXT,
    bg_third      INTEGER,
    PRIMARY KEY (user_id, name)
) WITHOUT ROWID;
