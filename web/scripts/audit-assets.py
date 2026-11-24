"""Checks that every student has the assets and metadata the pages use, and says who is missing what.

For each student in src/catalog.json it looks for: the portrait (collection image), icon, standing art, card
crops, lobby art, the face position, the halo, the school logo and the equipment slot types. Files are opened
and decoded, so a saved error page or a truncated download counts as missing. It also checks the shared
tables in public/meta.json. Exit code 1 when anything is missing, so a build can stop on it.

Run from web/ after `npm run assets`:  python -I scripts/audit-assets.py [--list]
"""
import json
import pathlib
import sys

import cv2
import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
PUB = ROOT / "public"
# Gaps in the sources themselves, not in this build: reported, but they do not fail the audit.
KNOWN_GAPS = {
    ("school logo", "사텐 루이코"),  # SchaleDB has no logo for Sakugawa Junior High
}
EQUIPMENT_TYPES = {"Hat", "Gloves", "Shoes", "Bag", "Badge", "Hairpin", "Charm", "Watch", "Necklace"}


def decodes(path: pathlib.Path) -> bool:
    """An image file that opens. A saved HTML page or a cut-off download does not."""
    if not path.exists() or path.stat().st_size < 64:
        return False
    data = np.fromfile(str(path), dtype=np.uint8)
    return cv2.imdecode(data, cv2.IMREAD_UNCHANGED) is not None


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    catalog = json.loads((ROOT / "src" / "catalog.json").read_text(encoding="utf-8"))
    faces = json.loads((PUB / "faces.json").read_text(encoding="utf-8")) if (PUB / "faces.json").exists() else {}
    halos = set(json.loads((PUB / "halos.json").read_text(encoding="utf-8"))) if (PUB / "halos.json").exists() else set()
    lobby_hi = set(json.loads((PUB / "card-lobby.json").read_text(encoding="utf-8"))) if (PUB / "card-lobby.json").exists() else set()
    meta = json.loads((PUB / "meta.json").read_text(encoding="utf-8")) if (PUB / "meta.json").exists() else {}
    backgrounds = json.loads((ROOT / "src" / "backgrounds.json").read_text(encoding="utf-8")) if (ROOT / "src" / "backgrounds.json").exists() else []

    checks = {
        "portrait": lambda s: decodes(PUB / "portraits" / f"{s['id']}.webp"),
        "icon": lambda s: decodes(PUB / "icons" / f"{s['id']}.webp"),
        "standing": lambda s: decodes(PUB / "standing" / f"{s['id']}.webp"),
        "card crop": lambda s: decodes(PUB / "card" / f"{s['id']}.webp"),
        "lobby crop": lambda s: decodes(PUB / "card-lobby" / f"{s['id']}.webp"),
        "face position": lambda s: str(s["id"]) in faces,
        "halo": lambda s: str(s["id"]) in halos or s["id"] in halos,
        "school logo": lambda s: bool(s.get("school")) and decodes(PUB / "schools" / f"{s['school']}.png"),
        "slot types": lambda s: isinstance(s.get("equipment"), list) and len(s["equipment"]) == 3 and set(s["equipment"]) <= EQUIPMENT_TYPES,
    }
    missing: dict[str, list[str]] = {k: [] for k in checks}
    for s in sorted(catalog.values(), key=lambda e: e["id"]):
        name = s["name"]
        for key, ok in checks.items():
            if not ok(s):
                missing[key].append(name)
    # halo files must be real PNGs, not saved error pages
    bad_halo = [p.name for p in (PUB / "halos").glob("*.png") if not decodes(p)] if (PUB / "halos").exists() else []

    total = len(catalog)
    print(f"{total} students")
    failed = False
    for key, names in missing.items():
        known = [n for n in names if (key, n) in KNOWN_GAPS]
        names = [n for n in names if (key, n) not in KNOWN_GAPS]
        mark = "ok" if not names else f"MISSING {len(names)}"
        print(f"  {key:14} {mark}" + (f" (source has none for: {', '.join(known)})" if known else ""))
        failed |= bool(names)
        if names and "--list" in sys.argv:
            print("    " + ", ".join(names))
    if bad_halo:
        print(f"  halo files that do not decode: {bad_halo}")
        failed = True
    print(f"  high resolution lobby art for {len(lobby_hi)} of {total}")
    shared = {
        "schools": bool(meta.get("schools")),
        "attack types": bool(meta.get("attack")),
        "armor types": bool(meta.get("armor")),
        "equipment names (9 types x 10 tiers)": sum(len(v) for v in meta.get("equipment", {}).values()) == 90,
        "character count": bool(meta.get("character_count")),
        "backgrounds": len(backgrounds) > 0 and all(decodes(PUB / "backgrounds" / f"{b}.jpg") for b in backgrounds),
    }
    for key, ok in shared.items():
        print(f"  {key:38} {'ok' if ok else 'MISSING'}")
        failed |= not ok
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
