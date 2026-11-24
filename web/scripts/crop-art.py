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
FACE_AT = 0.36  # where the face centre sits inside a standing crop, from the top
CARD_W, CARD_H = 720, 600  # size of every standing crop: the card shows the same picture size for every student


def detector():
    CACHE.mkdir(exist_ok=True)
    if not MODEL.exists():
        MODEL.write_bytes(urllib.request.urlopen(urllib.request.Request(MODEL_URL, headers={"User-Agent": "ba-key-amas-assets"}), timeout=60).read())
    # A low score threshold: anime faces in a wide scene (a beach picture with the figure sitting under an umbrella)
    # score 0.3 to 0.5. The best of the candidates is taken, not just any, see largest_face.
    return cv2.FaceDetectorYN.create(str(MODEL), "", (320, 320), 0.3, 0.3, 5000)


def largest_face(det, bgr):
    """The most likely face: the candidate with the best score times area, so a faint false hit does not beat a clear
    face. Returns (centre x, centre y, size) in pixels, or None."""
    h, w = bgr.shape[:2]
    det.setInputSize((w, h))
    _, faces = det.detect(bgr)
    if faces is None or len(faces) == 0:
        return None
    x, y, fw, fh = max(faces, key=lambda f: float(f[14]) * f[2] * f[3])[:4]
    return float(x + fw / 2), float(y + fh / 2), float(max(fw, fh))


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
    import json as _json
    faces, found = {}, 0
    for path in sorted((ROOT / "public" / "portraits").glob("*.webp")):
        bgr = cv2.imdecode(np.fromfile(str(path), dtype=np.uint8), cv2.IMREAD_COLOR)
        h, w = bgr.shape[:2]
        face = largest_face(det, bgr)
        if face is None:
            big = cv2.resize(bgr, None, fx=2, fy=2, interpolation=cv2.INTER_CUBIC)
            face = largest_face(det, big)
            if face is not None:
                face = (face[0] / 2, face[1] / 2, face[2] / 2)
        if face is not None:
            found += 1
            xy = (min(max(face[0] / w, 0.2), 0.8), min(max(face[1] / h, 0.25), 0.7))
        else:
            xy = MEAN_FACE
        faces[path.stem] = [round(float(xy[0]), 3), round(float(xy[1]), 3)]
    (ROOT / "public" / "faces.json").write_text(_json.dumps(faces), encoding="utf-8")
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
