"""Crops every student's card artwork once, in advance, around the face.

The card shows its artwork in a 6:5 window. Standing art (tall) and memorial lobby art (mostly wide) are cut to
that shape here, so the page never crops at runtime and a face is never cut in half.
  public/standing/<id>.webp -> public/card/<id>.webp         (614x512, head and shoulders)
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


def detector():
    CACHE.mkdir(exist_ok=True)
    if not MODEL.exists():
        MODEL.write_bytes(urllib.request.urlopen(urllib.request.Request(MODEL_URL, headers={"User-Agent": "ba-key-amas-assets"}), timeout=60).read())
    return cv2.FaceDetectorYN.create(str(MODEL), "", (320, 320), 0.5, 0.3, 5000)


def largest_face(det, bgr):
    h, w = bgr.shape[:2]
    det.setInputSize((w, h))
    _, faces = det.detect(bgr)
    if faces is None or len(faces) == 0:
        return None
    x, y, fw, fh = max(faces, key=lambda f: f[2] * f[3])[:4]
    return x + fw / 2, y + fh / 2, max(fw, fh)


def clamp(v, lo, hi):
    return max(lo, min(hi, v))


def crop_standing(img, face):
    w, h = img.size
    ch = round(w / RATIO)
    cy = face[1] if face else 0.2 * h
    top = clamp(round(cy - FACE_AT * ch), 0, max(h - ch, 0))
    return img.crop((0, top, w, top + ch)), (w, ch)


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
