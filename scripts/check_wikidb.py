#!/usr/bin/env python3
"""Fails when a freshly built wikidb/wiki.db is not one the add-on will accept or is missing a data group.

    python -I scripts/check_wikidb.py [wikidb/wiki.db]

The builder skips the story tables when their source is unreachable and still exits 0, so a release build must
check them here instead of shipping a degraded database. Standard library only.
"""
import pathlib
import re
import sqlite3
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
REQUIRED = ("student", "enemy", "raid", "story_episode", "event", "equipment_item", "student_equipment_slot")


def main() -> int:
    path = pathlib.Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "wikidb" / "wiki.db"
    lib = (ROOT / "scanner" / "crates" / "wikidb" / "src" / "lib.rs").read_text(encoding="utf-8")
    want = re.search(r"pub const SCHEMA_VERSION: u32 = (\d+);", lib).group(1)
    con = sqlite3.connect(f"file:{path.as_posix()}?mode=ro", uri=True)
    problems = []
    got = con.execute("SELECT value FROM meta WHERE key = 'schema_version'").fetchone()
    if not got or got[0] != want:
        problems.append(f"schema_version {got[0] if got else None}, the add-on reads {want}")
    for table in REQUIRED:
        n = con.execute(f"SELECT COUNT(*) FROM {table}").fetchone()[0]
        print(f"{table}: {n}")
        if n == 0:
            problems.append(f"table {table} is empty")
    for p in problems:
        print("FAIL:", p)
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
