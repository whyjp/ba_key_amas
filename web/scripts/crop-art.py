"""Crops every student's card artwork once, in advance, around the face.

The card shows its artwork in a 6:5 window. Standing art (tall) and memorial lobby art (mostly wide) are cut to
that shape here, so the page never crops at runtime and a face is never cut in half.
  public/standing/<id>.webp -> public/card/<id>.webp         (720x600, the face side of the figure)
  public/lobby/<id>.webp    -> public/card-lobby/<id>.webp   (480x400, upscaled and lightly sharpened)

Faces are found with OpenCV's YuNet detector (the model is downloaded once to scripts/.cache). It finds about
60 percent of anime faces; for the rest the crop falls back to where the face usually is (the upper part of
standing art, the middle of a lobby picture). The window never leaves the picture.

Run from web/ after extract-assets.py:  python -I scripts/crop-art.py   (needs opencv-python-headless and pillow)
"""
import json
import pathlib
import sys
import urllib.request

import cv2
import numpy as np
from PIL import Image, ImageFilter

ROOT = pathlib.Path(__file__).resolve().parent.parent
CACHE = ROOT / "scripts" / ".cache"
MODEL = CACHE / "yunet.onnx"
MODEL_URL = "https://github.com/opencv/opencv_zoo/raw/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx"
RATIO = 6 / 5
# Faces placed by hand where neither the detector nor the matching is right: {"standing"|"lobby": {id: [x, y]}} as
# fractions of the picture. Kept short and checked by eye (see scripts/face-overrides.json).
OVERRIDES = json.loads((ROOT / "scripts" / "face-overrides.json").read_text(encoding="utf-8")) if (ROOT / "scripts" / "face-overrides.json").exists() else {}
FACE_AT = 0.36  # where the face centre sits inside a standing crop, from the top
CARD_W, CARD_H = 720, 600  # size of every standing crop: the card shows the same picture size for every student


def detector():
    CACHE.mkdir(exist_ok=True)
    if not MODEL.exists():
        MODEL.write_bytes(urllib.request.urlopen(urllib.request.Request(MODEL_URL, headers={"User-Agent": "ba-key-amas-assets"}), timeout=60).read())
    # Threshold 0.3 finds faint anime faces but also hits hands, hems and props, so it only runs on a small window
    # that is already known to hold the head (face_near). On a whole picture a hit has to score 0.5 (largest_face).
    return cv2.FaceDetectorYN.create(str(MODEL), "", (320, 320), 0.3, 0.3, 5000)


def largest_face(det, bgr, min_score=0.5):
    """The most likely face: the candidate with the best score times area, so a faint false hit does not beat a clear
    face. Returns (centre x, centre y, size) in pixels, or None."""
    h, w = bgr.shape[:2]
    det.setInputSize((w, h))
    _, faces = det.detect(bgr)
    if faces is None:
        return None
    faces = [f for f in faces if float(f[14]) >= min_score]
    if not faces:
        return None
    x, y, fw, fh = max(faces, key=lambda f: float(f[14]) * f[2] * f[3])[:4]
    return float(x + fw / 2), float(y + fh / 2), float(max(fw, fh))


def locate_head(img, portrait_path):
    """Where the game's own head portrait sits inside the standing art: multi-scale template matching of the
    portrait's middle on the figure (the art is laid on white, the portrait has its own backdrop, so only the figure
    matches). Returns (score, centre x, centre y, region width) in standing-art pixels, or None."""
    if not portrait_path.exists():
        return None
    st = img.convert("RGBA")
    flat = Image.new("RGBA", st.size, (255, 255, 255, 255))
    flat.alpha_composite(st)
    big = cv2.cvtColor(np.array(flat.convert("RGB")), cv2.COLOR_RGB2GRAY)
    pt = cv2.cvtColor(np.array(Image.open(portrait_path).convert("RGB")), cv2.COLOR_RGB2GRAY)
    h, w = big.shape
    th, tw = pt.shape
    core = pt[int(th * 0.2):int(th * 0.8), int(tw * 0.2):int(tw * 0.8)]
    k = min(1.0, 800 / max(h, w))
    small = cv2.resize(big, None, fx=k, fy=k, interpolation=cv2.INTER_AREA)
    best = None
    for region_w in np.linspace(120, min(w, h), 40):  # width of the portrait's region inside the standing art
        s = (region_w / tw) * k
        cw, ch = int(core.shape[1] * s), int(core.shape[0] * s)
        if cw < 20 or ch < 20 or cw >= small.shape[1] or ch >= small.shape[0]:
            continue
        res = cv2.matchTemplate(small, cv2.resize(core, (cw, ch), interpolation=cv2.INTER_AREA), cv2.TM_CCOEFF_NORMED)
        _, score, _, loc = cv2.minMaxLoc(res)
        if best is None or score > best[0]:
            best = (float(score), (loc[0] + cw / 2) / k, (loc[1] + ch / 2) / k, float(region_w))
    return best


ANCHOR_MIN = 0.55  # below this the match is not trusted


def face_near(det, bgr, anchor):
    """The face inside the window around the matched head, found with the low threshold; the match centre when the
    detector finds nothing there. (x, y, size) in standing-art pixels."""
    _, cx, cy, rw = anchor
    h, w = bgr.shape[:2]
    half = rw * 0.8
    x0, y0 = int(max(cx - half, 0)), int(max(cy - half, 0))
    x1, y1 = int(min(cx + half, w)), int(min(cy + half, h))
    window = bgr[y0:y1, x0:x1]
    found = largest_face(det, window, min_score=0.3) if window.size else None
    if found:
        return found[0] + x0, found[1] + y0, found[2]
    return cx, cy, rw * 0.5


def clamp(v, lo, hi):
    return max(lo, min(hi, v))


def standing_window(img, face, ratio, height_share, face_at, size):
    """The face side of the vertical portrait, cut to a window of `ratio` (width over height).

    The standing art is a tall cut-out with transparent margins, and the figure often fills only the middle of its
    width, so a full-width window shows empty sides. The window here is sized from the figure instead: its height is
    `height_share` of the figure (head and shoulders), its width follows from the ratio, it is centred on the face and
    the face sits `face_at` of the way down. The result is scaled to a fixed size, so every student's picture is
    cut the same way and shown at the same size."""
    w, h = img.size
    alpha = np.array(img.getchannel("A")) > 24
    rows = np.where(alpha.any(axis=1))[0]
    y0, y1 = (int(rows[0]), int(rows[-1])) if rows.size else (0, h - 1)
    fig_h = max(y1 - y0, 1)
    head = alpha[y0:y0 + max(int(fig_h * 0.22), 1)]
    cols = np.where(head.any(axis=0))[0]
    if face:
        cx, cy = face[0], face[1]
    elif h >= 1.3 * w:
        # a tall figure: the head is at the top
        cx, cy = (float(cols.mean()) if cols.size else w / 2), y0 + 0.14 * fig_h
    else:
        # a scene (the figure sits among props): the top of the picture is not the head, so aim at the middle
        cx, cy = w / 2, y0 + 0.38 * fig_h
    wh = fig_h * height_share
    if face:
        wh = max(wh, face[2] * 2.8)
    wh = min(wh, h, w / ratio)
    ww = wh * ratio
    top = clamp(round(cy - face_at * wh), 0, max(round(h - wh), 0))
    left = clamp(round(cx - ww / 2), 0, max(round(w - ww), 0))
    return img.crop((left, top, left + round(ww), top + round(wh))), size


def crop_standing(img, face):
    """The card's 6:5 picture window. The student tiles keep the game's own portrait, which is not cut here."""
    return standing_window(img, face, RATIO, 0.34, FACE_AT, (CARD_W, CARD_H))


def crop_lobby(img, face):
    """The lobby thumbnails are masked (transparent corners). Take the largest 6:5 window that is fully opaque,
    nearest the face, so no transparent pixel reaches the card."""
    opaque = (np.array(img.convert("RGBA"))[..., 3] > 200).astype(np.uint8)
    h, w = opaque.shape
    ii = cv2.integral(opaque)
    cx, cy = (face[0], face[1]) if face else (w / 2, h * 0.45)

    def solid(left, top, cw, ch):
        return ii[top + ch, left + cw] - ii[top, left + cw] - ii[top + ch, left] + ii[top, left] == cw * ch

    for scale in np.linspace(1.0, 0.35, 27):
        ch = int(h * scale)
        cw = int(ch * RATIO)
        if cw > w:
            cw, ch = w, int(w / RATIO)
        best = None
        for top in range(0, h - ch + 1, 2):
            for left in range(0, w - cw + 1, 2):
                if solid(left, top, cw, ch):
                    d = (left + cw / 2 - cx) ** 2 + (top + ch * 0.42 - cy) ** 2
                    if best is None or d < best[0]:
                        best = (d, left, top)
        if best:
            _, left, top = best
            return img.convert("RGB").crop((left, top, left + cw, top + ch)), (480, 400)
    cw = min(w, round(h * RATIO))
    ch = round(cw / RATIO)
    return img.convert("RGB").crop(((w - cw) // 2, (h - ch) // 2, (w - cw) // 2 + cw, (h - ch) // 2 + ch)), (480, 400)


# The portraits (public/portraits, 200x226) are cut with the face near the middle: across the students the detected
# face centres sit at x 0.51 and y 0.48 with little spread. A student whose face is not found takes that mean.
MEAN_FACE = (0.51, 0.48)


def write_faces(det) -> int:
    """`public/faces.json`: id -> [x, y], the face centre as fractions of the portrait, for placing the face in a
    heatmap cell. Found at normal size, then at double size (small faces), else the mean."""
    faces, found = {}, 0
    for path in sorted((ROOT / "public" / "portraits").glob("*.webp")):
        bgr = cv2.imdecode(np.fromfile(str(path), dtype=np.uint8), cv2.IMREAD_COLOR)
        h, w = bgr.shape[:2]
        face = largest_face(det, bgr, min_score=0.3)
        if face is None:
            big = cv2.resize(bgr, None, fx=2, fy=2, interpolation=cv2.INTER_CUBIC)
            face = largest_face(det, big, min_score=0.3)
            if face is not None:
                face = (face[0] / 2, face[1] / 2, face[2] / 2)
        if face is not None:
            found += 1
            xy = (min(max(face[0] / w, 0.2), 0.8), min(max(face[1] / h, 0.25), 0.7))
        else:
            xy = MEAN_FACE
        faces[path.stem] = [round(float(xy[0]), 3), round(float(xy[1]), 3)]
    (ROOT / "public" / "faces.json").write_text(json.dumps(faces), encoding="utf-8")
    return found


def main() -> int:
    det = detector()
    jobs = (("standing", "card", crop_standing, 88), ("lobby", "card-lobby", crop_lobby, 90))
    done = found = 0
    for src, dst, fn, quality in jobs:
        out = ROOT / "public" / dst
        out.mkdir(parents=True, exist_ok=True)
        for path in sorted((ROOT / "public" / src).glob("*.webp")):
            target = out / path.name
            if target.exists():
                continue
            bgr = cv2.imdecode(np.fromfile(str(path), dtype=np.uint8), cv2.IMREAD_COLOR)
            override = OVERRIDES.get(src, {}).get(path.stem)
            if override:
                face = (override[0] * bgr.shape[1], override[1] * bgr.shape[0], bgr.shape[1] * 0.12)
            elif src == "standing":
                head = locate_head(Image.open(path), ROOT / "public" / "portraits" / path.name)
                face = face_near(det, bgr, head) if head and head[0] >= ANCHOR_MIN else largest_face(det, bgr)
            else:
                face = largest_face(det, bgr)
            found += face is not None
            # standing art is cut out (transparent), so its alpha is kept and the card paints the backdrop
            img = Image.open(path).convert("RGBA")
            cropped, size = fn(img, face)
            if cropped.size != size:
                cropped = cropped.resize(size, Image.LANCZOS)
                if src == "lobby":
                    cropped = cropped.filter(ImageFilter.UnsharpMask(radius=1.2, percent=60, threshold=2))
            cropped.save(target, "WEBP", quality=quality)
            done += 1
    print(f"{done} crops written, face found in {found}")
    print(f"faces.json: face found on {write_faces(det)} portraits, the rest take the mean")
    return 0


if __name__ == "__main__":
    sys.exit(main())
