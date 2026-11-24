-- The rest of what the scan reads per student (stats, equipment, terrain, ...), already validated, as JSON.
ALTER TABLE students ADD COLUMN detail_json TEXT;
