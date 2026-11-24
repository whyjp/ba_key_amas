-- The background a student's card was given when the student was first uploaded (kept on later uploads),
-- and which third of it shows: 0 left, 1 centre, 2 right.
ALTER TABLE students ADD COLUMN bg TEXT;
ALTER TABLE students ADD COLUMN bg_third INTEGER;
