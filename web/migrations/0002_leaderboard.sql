-- Leaderboard opt-in and the numbers the leaderboard sorts on, computed when a scan is uploaded.
ALTER TABLE users ADD COLUMN public INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN score REAL NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN stats_json TEXT;      -- counts shown as leaderboard columns
ALTER TABLE users ADD COLUMN top_json TEXT;        -- the three best characters, each with its outfits
ALTER TABLE users ADD COLUMN score_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE students ADD COLUMN memorial INTEGER;  -- memorial lobby unlocked (from the duty menu scan), null when unknown
CREATE INDEX users_board ON users (public, score DESC);
