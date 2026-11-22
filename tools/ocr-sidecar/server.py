"""DEV-ONLY reference OCR sidecar (PP-OCR via RapidOCR).

The Go program talks to this over localhost HTTP. It exists to produce golden
outputs and to validate the PP-OCR backend on real data; the shipped backend is
meant to run in-process in Go. Pinned configuration (what was measured):

    detection   : RapidOCR default, PP-OCRv6 det small
    recognition : korean PP-OCRv5 mobile
    classifier  : ch_ppocr_mobile_v2.0_cls_mobile (angle, unused for horizontal UI text)

Run:  pip install rapidocr onnxruntime  &&  python server.py [--port 47321]

POST /boxes   body = PNG frame        -> [{"x0","y0","x1","y1","text","score"}, ...]
POST /line    body = PNG text line    -> {"text","score"}   (recognition only, no detection)
GET  /health  -> {"ok": true, "models": {...}}
"""
import argparse
import io
import json
from http.server import BaseHTTPRequestHandler, HTTPServer

import numpy as np
from PIL import Image
from rapidocr import LangRec, ModelType, OCRVersion, RapidOCR

PARAMS = {
    "Rec.lang_type": LangRec.KOREAN,
    "Rec.ocr_version": OCRVersion.PPOCRV5,
    "Rec.model_type": ModelType.MOBILE,
}
# Two instances: a recognition-only call (use_det=False) changes the engine's
# state, after which detection calls on the same instance return no boxes.
ENGINE = RapidOCR(params=PARAMS)       # detection + recognition (frames)
LINE_ENGINE = RapidOCR(params=PARAMS)  # recognition only (pre-cropped lines)
LINE_SCALE = 4  # tight crops are upscaled before recognition (matches the benchmark)


def boxes(img: Image.Image):
    res = ENGINE(np.array(img.convert("RGB")))
    out = []
    if getattr(res, "boxes", None) is None:  # nothing detected: RapidOCR returns a rec-only output
        return out
    for pts, text, score in zip(res.boxes, res.txts, res.scores):
        pts = np.array(pts)
        out.append({"x0": float(pts[:, 0].min()), "y0": float(pts[:, 1].min()),
                    "x1": float(pts[:, 0].max()), "y1": float(pts[:, 1].max()),
                    "text": text, "score": float(score)})
    return out


def line(img: Image.Image):
    img = img.convert("RGB")
    img = img.resize((img.width * LINE_SCALE, img.height * LINE_SCALE), Image.LANCZOS)
    res = LINE_ENGINE(np.array(img), use_det=False, use_cls=False, use_rec=True)
    text = "".join(res.txts) if res.txts else ""
    score = float(np.mean(res.scores)) if res.scores else 0.0
    return {"text": text, "score": score}


class Handler(BaseHTTPRequestHandler):
    def _send(self, obj, code=200):
        body = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            self._send({"ok": True, "models": {"det": "PP-OCRv6 det small", "rec": "korean PP-OCRv5 mobile"}})
        else:
            self._send({"error": "not found"}, 404)

    def do_POST(self):
        n = int(self.headers.get("Content-Length", 0))
        try:
            img = Image.open(io.BytesIO(self.rfile.read(n)))
            if self.path == "/boxes":
                self._send(boxes(img))
            elif self.path == "/line":
                self._send(line(img))
            else:
                self._send({"error": "not found"}, 404)
        except Exception as e:  # report, never crash the server
            self._send({"error": str(e)}, 500)

    def log_message(self, *a):  # quiet
        pass


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--port", type=int, default=47321)
    args = ap.parse_args()
    HTTPServer.allow_reuse_address = False  # fail loudly instead of sharing a port with another process
    HTTPServer(("127.0.0.1", args.port), Handler).serve_forever()
