"""Where the memorial card shows the lobby picture, per student (public/card-lobby-focus.json).

A full illustration card draws the 760x1280 lobby cut over its top 54 percent, so only the band from 10 to 64 percent of the
picture's height shows (CSS `object-position: center 22%`). A face outside that band is cut by the card, whatever the crop
did. This finds each face with the anime face detector and, where the default band does not hold it with a margin, writes
the object-position (in percent) that moves the band onto the face. Pictures whose face is already inside get no entry.

    python -I scripts/lobby-focus.py
"""
import importlib.util
import json
import pathlib

import cv2
import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
SHOWN = 0.54            # the share of the picture's height the card shows
DEFAULT = 0.22          # the object-position the style sheet uses
MARGIN = 0.03


def main():
    spec = importlib.util.spec_from_file_location("anime_faces", ROOT / "scripts" / "anime_faces.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    det = mod.load()
    room = 1 - SHOWN
    focus = {}
    for path in sorted((ROOT / "public" / "card-lobby").glob("*.webp")):
        bgr = cv2.imdecode(np.fromfile(str(path), dtype=np.uint8), cv2.IMREAD_COLOR)
        found = det.detect(bgr)
        if not found:
            continue
        h = bgr.shape[0]
        top, bottom = found[0][1] / h, (found[0][1] + found[0][3]) / h
        start = DEFAULT * room
        if top - MARGIN >= start and bottom + MARGIN <= start + SHOWN:
            continue
        # the band's start that holds the whole face if it can, else the face's top (the eyes matter more than the chin)
        want = min(max(bottom + MARGIN - SHOWN, 0.0), top - MARGIN)
        focus[path.stem] = round(min(max(want, 0.0), room) / room * 100)
    (ROOT / "public" / "card-lobby-focus.json").write_text(json.dumps(focus, sort_keys=True), encoding="utf-8")
    print(f"{len(focus)} pictures need a moved band")


if __name__ == "__main__":
    main()
