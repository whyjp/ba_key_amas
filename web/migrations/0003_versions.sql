-- Which add-on and contract produced the stored scan.
ALTER TABLE users ADD COLUMN addon_version TEXT;
ALTER TABLE users ADD COLUMN protocol INTEGER;
