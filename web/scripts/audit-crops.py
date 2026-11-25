"""Looks over the card pictures for cut faces and for faces that fill too much of the picture.

    python -I scripts/audit-crops.py [--sheets DIR]

For every picture in public/card-lobby (memorial lobby art, tall) and public/card (standing art, 6:5) the face is found
with the YuNet detector (the model crop-art.py downloads) and these are reported:

  cut     the face box leaves the picture or comes within 1.5 percent of its edge
  tight   the face is taller than TIGHT_SHARE of the picture (zoomed in too far for a card)
  none    no face found (the picture has to be looked at: about 40 percent of anime faces are missed)

`--sheets DIR` also writes contact sheets (24 pictures each, numbered, flagged ones framed in red) to be looked at by eye.
Detection misses and false hits are expected: the report picks what to look at first, it does not decide.
"""
import argparse
import importlib.util
import json
import pathlib

import cv2
import numpy as np
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent
TIGHT_SHARE = {"card-lobby": 0.30, "card": 0.62}
EDGE = 0.015


def load_cropper():
    spec = importlib.util.spec_from_file_location("crop_art", ROOT / "scripts" / "crop-art.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def audit(folder, det, mod):
    rows = []
    for path in sorted((ROOT / "public" / folder).glob("*.webp")):
        bgr = cv2.imdecode(np.fromfile(str(path), dtype=np.uint8), cv2.IMREAD_COLOR)
        h, w = bgr.shape[:2]
        face = mod.largest_face(det, bgr, min_score=0.3)
        row = {"id": path.stem, "size": [w, h], "flags": []}
        if face is None:
            row["flags"].append("none")
        else:
            cx, cy, size = face
            half = size / 2
            row["face"] = [round(cx / w, 3), round(cy / h, 3), round(size / h, 3)]
            if cx - half < EDGE * w or cx + half > (1 - EDGE) * w or cy - half < EDGE * h or cy + half > (1 - EDGE) * h:
                row["flags"].append("cut")
            if size / h > TIGHT_SHARE[folder]:
                row["flags"].append("tight")
        rows.append(row)
    return rows


def sheets(folder, rows, out):
    out.mkdir(parents=True, exist_ok=True)
    cell_w = 190 if folder == "card-lobby" else 240
    cell_h = 320 if folder == "card-lobby" else 200
    per_row = 8 if folder == "card-lobby" else 6
    for n in range(0, len(rows), 24):
        chunk = rows[n:n + 24]
        sheet = Image.new("RGB", (per_row * cell_w, ((len(chunk) + per_row - 1) // per_row) * (cell_h + 18)), (30, 30, 30))
        d = ImageDraw.Draw(sheet)
        for i, row in enumerate(chunk):
            x, y = (i % per_row) * cell_w, (i // per_row) * (cell_h + 18)
            im = Image.open(ROOT / "public" / folder / f"{row['id']}.webp").convert("RGB").resize((cell_w - 4, cell_h), Image.LANCZOS)
            sheet.paste(im, (x + 2, y + 18))
            colour = (255, 70, 70) if row["flags"] else (200, 200, 200)
            d.text((x + 4, y + 3), f"{row['id']} {','.join(row['flags'])}", fill=colour)
            if row["flags"]:
                d.rectangle((x + 1, y + 17, x + cell_w - 2, y + 18 + cell_h), outline=(255, 70, 70), width=2)
        sheet.save(out / f"{folder}-{n // 24 + 1:02d}.png")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--sheets")
    a = ap.parse_args()
    mod = load_cropper()
    det = mod.detector()
    report = {}
    for folder in ("card-lobby", "card"):
        rows = audit(folder, det, mod)
        report[folder] = rows
        counts = {k: sum(k in r["flags"] for r in rows) for k in ("cut", "tight", "none")}
        print(f"{folder}: {len(rows)} pictures, {counts}")
        if a.sheets:
            sheets(folder, rows, pathlib.Path(a.sheets))
    (pathlib.Path(a.sheets) / "report.json").write_text(json.dumps(report, ensure_ascii=False), encoding="utf-8") if a.sheets else None


if __name__ == "__main__":
    main()
