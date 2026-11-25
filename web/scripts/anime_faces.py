"""Anime face detector for the crop audit: deepghs/anime_face_detection (YOLOv8 trained on anime and game art, ONNX, 44 MB,
downloaded once to scripts/.cache and not committed). The YuNet detector crop-art.py uses is trained on photographs and
finds the face on fewer than one memorial lobby picture in six; this one finds it on all 270.
"""
import pathlib
import urllib.request

import cv2
import numpy as np
import onnxruntime as ort

MODEL_URL = "https://huggingface.co/deepghs/anime_face_detection/resolve/main/face_detect_v1.4_s/model.onnx"
MODEL = pathlib.Path(__file__).resolve().parent / ".cache" / "anime_face_v1.4_s.onnx"


def load():
    if not MODEL.exists():
        MODEL.parent.mkdir(parents=True, exist_ok=True)
        urllib.request.urlretrieve(MODEL_URL, MODEL)
    return AnimeFaces(str(MODEL))


class AnimeFaces:
    """deepghs/anime_face_detection (YOLOv8, ONNX): boxes (x, y, w, h, score) in the picture's own pixels."""
    def __init__(self, path, size=640):
        self.s = ort.InferenceSession(path, providers=["CPUExecutionProvider"])
        self.size = size
        self.name = self.s.get_inputs()[0].name

    def detect(self, bgr, conf=0.3, iou=0.5):
        h, w = bgr.shape[:2]
        r = self.size / max(h, w)
        nh, nw = round(h * r), round(w * r)
        canvas = np.full((self.size, self.size, 3), 114, np.uint8)
        top, left = (self.size - nh) // 2, (self.size - nw) // 2
        canvas[top:top + nh, left:left + nw] = cv2.resize(bgr, (nw, nh), interpolation=cv2.INTER_AREA)
        x = canvas[:, :, ::-1].transpose(2, 0, 1)[None].astype(np.float32) / 255.0
        out = self.s.run(None, {self.name: x})[0][0]  # (4 + classes, N)
        out = out.T
        scores = out[:, 4:].max(1)
        keep = scores >= conf
        out, scores = out[keep], scores[keep]
        if not len(out):
            return []
        cx, cy, bw, bh = out[:, 0], out[:, 1], out[:, 2], out[:, 3]
        boxes = np.stack([cx - bw / 2, cy - bh / 2, bw, bh], 1)
        idx = cv2.dnn.NMSBoxes(boxes.tolist(), scores.tolist(), conf, iou)
        res = []
        for i in np.array(idx).reshape(-1):
            bx, by, bw_, bh_ = boxes[i]
            res.append(tuple(float(v) for v in ((bx - left) / r, (by - top) / r, bw_ / r, bh_ / r, scores[i])))
        return sorted(res, key=lambda b: -b[4] * b[2] * b[3])
