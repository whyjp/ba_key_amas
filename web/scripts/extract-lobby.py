"""High resolution memorial lobby art for the full-illustration cards (public/card-lobby/<id>.webp, 760x1280).

Source: the memorial lobby images (`File:Memorial_Lobby_<Name>.jpg`) of the Blue Archive wiki (bluearchive.wiki),
listed through its MediaWiki API and hosted on static.wikitide.net, 1.5 to 2.5 thousand pixels wide. Each image is
matched to a student through the English name in SchaleDB (the wiki writes some outfits differently, see
SYNONYMS), then cut to the card's portrait shape around the face (a detected face first, else the spot where SchaleDB's
face-centred thumbnail sits in the big picture, found by template matching). The originals are cached in scripts/.cache and
nothing is committed. Students without a wiki image keep the small SchaleDB lobby crop made by crop-art.py.
`public/card-lobby.json` lists the ids that got the high resolution picture.

Rights stay with the wiki's contributors and the game's rights holders (Nexon, Nexon Games, Yostar).

Run from web/ after extract-assets.py and before crop-art.py:  python -I scripts/extract-lobby.py [--force]
"""
import importlib.util
import json
import pathlib
import re
import sys
import time
import urllib.parse
import urllib.request

import cv2
import numpy as np
from PIL import Image

ROOT = pathlib.Path(__file__).resolve().parent.parent
CACHE = ROOT / "scripts" / ".cache" / "lobby"
API = "https://bluearchive.wiki/w/api.php"
UA = "ba-key-amas-assets (build-time, one request per image)"
PAUSE = 0.25
OUT_W, OUT_H = 760, 1280  # the card is 380x640, drawn at twice the size
RATIO = OUT_W / OUT_H

# SchaleDB's English outfit word -> the wiki's
SYNONYMS = {
    "bunny": "bunny girl", "school": "school uniform", "armed": "battle", "camp": "camping", "cheer squad": "cheerleader",
    "pop idol": "idol", "track": "sportswear", "pajamas": "pajama", "cycling": "riding", "small": "kid",
}


def get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=90) as r:
                return r.read()
        except OSError:
            if attempt == 4:
                raise
            time.sleep(2 * (attempt + 1))
    raise RuntimeError("unreachable")


def norm(s: str) -> str:
    return re.sub(r"[^a-z0-9]", "", s.lower())


def wiki_images() -> dict[str, dict]:
    """`norm(name) -> file` for the plain memorial lobby images (numbered, pre-release and dated ones are skipped)."""
    out: dict[str, dict] = {}
    cont = None
    while True:
        q = {"action": "query", "list": "allimages", "aiprefix": "Memorial_Lobby_", "ailimit": "500", "aiprop": "url|size", "format": "json"}
        if cont:
            q["aicontinue"] = cont
        data = json.loads(get(API + "?" + urllib.parse.urlencode(q)))
        for f in data["query"]["allimages"]:
            name = f["name"][len("Memorial_Lobby_"):].rsplit(".", 1)[0].replace("_", " ")
            if re.search(r"( \d+|pre-release.*|\(\d{4}\))$", name, re.I):
                continue
            key = norm(name)
            if key not in out or f["width"] * f["height"] > out[key]["width"] * out[key]["height"]:
                out[key] = f
        cont = data.get("continue", {}).get("aicontinue")
        if not cont:
            return out


def wiki_key(english: str) -> str:
    m = re.match(r"^(.*?)\s*\((.*)\)$", english)
    base, outfit = (m.group(1), m.group(2)) if m else (english, "")
    base = {"Aris": "Arisu"}.get(base, base)
    outfit = SYNONYMS.get(outfit.lower(), outfit)
    return norm(f"{base} ({outfit})" if outfit else base)


def load_crop_art():
    spec = importlib.util.spec_from_file_location("crop_art", ROOT / "scripts" / "crop-art.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


THUMB_TRUST = 0.85  # a thumbnail match this good is believed over the face detector


def locate_by_thumbnail(big_gray, thumb_path: pathlib.Path):
    """Find where SchaleDB's small lobby crop (cut around the character's face) sits inside the big picture.

    Multi-scale template matching on the opaque middle of the thumbnail. Returns (score, cx, cy) in big-image pixels.
    """
    if not thumb_path.exists():
        return 0.0, None, None
    t = np.array(Image.open(thumb_path).convert("RGBA"))
    th, tw = t.shape[:2]
    core = cv2.cvtColor(t[:, :, :3], cv2.COLOR_RGB2GRAY)[int(th * 0.15):int(th * 0.85), int(tw * 0.15):int(tw * 0.85)]
    h, w = big_gray.shape
    k = 640 / w
    small = cv2.resize(big_gray, (640, max(1, int(h * k))))
    best = (0.0, None, None)
    for crop_w in np.linspace(120, w, 40):  # width of the thumbnail's region in the big picture
        s = (crop_w / tw) * k
        cw, ch = int(core.shape[1] * s), int(core.shape[0] * s)
        if cw < 24 or ch < 24 or cw >= small.shape[1] or ch >= small.shape[0]:
            continue
        res = cv2.matchTemplate(small, cv2.resize(core, (cw, ch), interpolation=cv2.INTER_AREA), cv2.TM_CCOEFF_NORMED)
        _, score, _, loc = cv2.minMaxLoc(res)
        if score > best[0]:
            best = (float(score), (loc[0] + cw / 2) / k, (loc[1] + ch / 2) / k)
    return best


def portrait_crop(img: Image.Image, face, spot, shift: float = 0.0):
    """A full-height portrait window. Centre: the detected face, else the matched thumbnail's centre, else the
    middle. When a face box is known the window is nudged until the whole face sits inside with a margin.
    `shift` moves the window sideways afterwards, as a fraction of its width (positive = right): the hand-made correction
    for a picture whose face still sits at an edge (`lobby_shift` in face-overrides.json). Returns the picture and where
    the window lies, (left, width, picture width)."""
    w, h = img.size
    cw = min(w, round(h * RATIO))
    ch = round(cw / RATIO)
    cx = face[0] if face else (spot[0] if spot else w * 0.5)
    left = round(cx - cw / 2)
    if face:
        half = face[2] / 2
        margin = cw * 0.08
        left = min(left, round(cx - half - margin))      # keep the face's left edge inside
        left = max(left, round(cx + half + margin - cw))  # and its right edge
    left = round(left + shift * cw)
    left = max(0, min(left, w - cw))
    top = max(0, min(round(h - ch) // 2, h - ch))
    return img.crop((left, top, left + cw, top + ch)).resize((OUT_W, OUT_H), Image.LANCZOS), (left, cw, w)


def main() -> int:
    sys.stdout.reconfigure(encoding="utf-8")
    crop = load_crop_art()
    det = crop.detector()
    catalog = json.loads((ROOT / "src" / "catalog.json").read_text(encoding="utf-8"))
    ids = sorted({e["id"] for e in catalog.values()})
    english = json.loads(get("https://schaledb.com/data/en/students.min.json"))
    files = wiki_images()
    CACHE.mkdir(parents=True, exist_ok=True)
    out = ROOT / "public" / "card-lobby"
    out.mkdir(parents=True, exist_ok=True)
    # ids that already have a high resolution cut; --force makes them all again
    prev = ROOT / "public" / "card-lobby.json"
    done_before = set(json.loads(prev.read_text(encoding="utf-8"))) if prev.exists() else set()
    # --only 10129,20020 cuts just those again (the rest of card-lobby.json is kept)
    only = {int(x) for x in sys.argv[sys.argv.index("--only") + 1].split(",")} if "--only" in sys.argv else set()
    have, missing = [], []
    how = {"face": 0, "thumbnail": 0, "middle": 0}
    for sid in ids:
        name = english.get(str(sid), {}).get("Name", "")
        f = files.get(wiki_key(name))
        if not f:
            missing.append(name)
            continue
        if only and sid not in only:
            if sid in done_before:
                have.append(sid)
            continue
        if not only and (out / f"{sid}.webp").exists() and "--force" not in sys.argv and (out / f"{sid}.webp").stat().st_size > 5000 and sid in done_before:
            have.append(sid)
            continue
        ext = f["url"].rsplit(".", 1)[-1].lower()
        src = CACHE / f"{sid}.{ext}"
        if not src.exists():
            time.sleep(PAUSE)
            src.write_bytes(get(f["url"]))
        bgr = cv2.imdecode(np.fromfile(str(src), dtype=np.uint8), cv2.IMREAD_COLOR)
        if bgr is None:
            missing.append(name)
            continue
        h, w = bgr.shape[:2]
        by_hand = crop.OVERRIDES.get("lobby", {}).get(str(sid))
        face, spot = None, None
        if by_hand:
            spot = (by_hand[0] * w, by_hand[1] * h)
        else:
            # SchaleDB's own lobby crop is framed on the character, so where it sits in the big picture is the
            # surest guide; the face detector is the second choice (it also hits hands and props in a wide scene)
            score, cx, cy = locate_by_thumbnail(cv2.cvtColor(bgr, cv2.COLOR_BGR2GRAY), ROOT / "public" / "lobby" / f"{sid}.webp")
            if score >= THUMB_TRUST and cx is not None:
                spot = (cx, cy)
            else:
                face = crop.largest_face(det, bgr)
                if face is None and score >= 0.6 and cx is not None:
                    spot = (cx, cy)
        how[("face" if face else "thumbnail" if spot else "middle")] += 1
        img = Image.open(src).convert("RGB")
        shift = float(crop.OVERRIDES.get("lobby_shift", {}).get(str(sid), 0.0))
        cropped, (left, cw, w) = portrait_crop(img, face, spot, shift)
        cropped.save(out / f"{sid}.webp", "WEBP", quality=86)
        if only:
            print(f"{sid} {name}: picture {w}x{h}, window left {left} of {w - cw} possible, width {cw}, shift {shift}, centred by {'face' if face else 'thumbnail' if spot else 'middle'}")
        have.append(sid)
    (ROOT / "public" / "card-lobby.json").write_text(json.dumps(have), encoding="utf-8")
    print(f"{len(have)} high resolution lobby cards, {len(missing)} without: {', '.join(missing[:10])}. centred by {how}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
