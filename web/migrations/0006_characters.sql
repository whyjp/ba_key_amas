-- Each user's characters with their totals (outfits merged), kept at upload so the leaderboard can add
-- up the whole community's characters without reading every student row.
ALTER TABLE users ADD COLUMN chars_json TEXT;
