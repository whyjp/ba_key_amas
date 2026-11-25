"""Compares two student scans field by field: did a change to the scanner alter what it reads?

    python scripts/compare_scans.py old/students.json new/students.json

Lists every student whose read values differ (level, stars, skills, bond, weapon, stats, equipment, name), and the scan
duration when the detail folders are given with --details-old / --details-new (the modification times of the
student.json files, one per student, tell how long each took).
"""
import argparse
import json
import os
import statistics
import sys

FIELDS = ["name_match.candidate", "level", "rarity_stars", "ex_skill_level", "normal_skill_level", "passive_skill_level",
          "sub_skill_level", "bond_rank", "unique_weapon_stars", "unique_weapon_level", "stats", "exp", "equipment", "loadout"]


def dig(obj, path):
    for part in path.split("."):
        obj = obj.get(part) if isinstance(obj, dict) else None
    return obj


def load(path):
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
    return {e["id"]: e["student"] for e in data["entries"]}


def durations(folder):
    rows = sorted((int(d), os.path.getmtime(os.path.join(folder, d, "student.json"))) for d in os.listdir(folder) if os.path.exists(os.path.join(folder, d, "student.json")))
    gaps = [b[1] - a[1] for a, b in zip(rows, rows[1:])]
    return rows[-1][1] - rows[0][1], statistics.median(gaps)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("old")
    ap.add_argument("new")
    ap.add_argument("--details-old")
    ap.add_argument("--details-new")
    a = ap.parse_args()
    old, new = load(a.old), load(a.new)
    print(f"{len(old)} students before, {len(new)} after")
    bad = 0
    for sid in sorted(set(old) | set(new)):
        if sid not in old or sid not in new:
            print(f"{sid}: only in {'old' if sid in old else 'new'}")
            bad += 1
            continue
        diffs = [(f, dig(old[sid], f), dig(new[sid], f)) for f in FIELDS if dig(old[sid], f) != dig(new[sid], f)]
        if diffs:
            bad += 1
            name = dig(new[sid], "name_match.candidate")
            print(f"{sid} {name}")
            for f, o, n in diffs:
                print(f"    {f}: {o!r} -> {n!r}")
    print("identical" if not bad else f"{bad} students differ")
    if a.details_old and a.details_new:
        (to, mo), (tn, mn) = durations(a.details_old), durations(a.details_new)
        print(f"duration {to:.0f}s ({mo:.1f}s per student) -> {tn:.0f}s ({mn:.1f}s per student)")
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
