"""How long the add-on takes to show a new state in 빠른 편성 after the player acts. Needs the live game, open on 빠른 편성
(STRIKER tab, first row, the fourth card selected) and moves the real mouse (a click on that card and wheel turns over the grid;
every action is undone by its pair).

    python scripts/overlay-latency.py <add-on exe> [--trials 6] [--models DIR] [--wiki DB] [--out result.json]

Per action it records
    change   when the game's picture first changed after the input (the game's own response time)
    settled  when the picture stopped moving (a floor: nothing can be read before it)
    shown    when the add-on had worked out the new state (a new `assist:` line with another squad or other cards)
and prints the median and the slowest of `shown` and of `shown - settled` (what the add-on adds). The add-on is started fresh.
The error count (actions that showed nothing within 8 s) is printed with the number of actions made.
"""
import argparse
import ctypes
import json
import os
import re
import statistics
import subprocess
import sys
import threading
import time

from PIL import ImageGrab

u = ctypes.windll.user32
u.SetProcessDPIAware()


def game_rect():
    h = u.FindWindowW(None, "Blue Archive")
    from ctypes import wintypes
    r = wintypes.RECT()
    pt = wintypes.POINT(0, 0)
    u.GetClientRect(h, ctypes.byref(r))
    u.ClientToScreen(h, ctypes.byref(pt))
    return pt.x, pt.y, r.right, r.bottom


def look(box):
    return list(ImageGrab.grab(bbox=box, all_screens=True).convert("L").resize((48, 36)).getdata())


def dist(a, b):
    return sum(abs(x - y) for x, y in zip(a, b)) / len(a)


def squad_of(line):
    m = re.search(r"squad=\[([^\]]*)\]", line)
    return m.group(1) if m else ""


def cards_of(line):
    m = re.search(r"cards=\[([^\]]*)\]", line)
    return m.group(1) if m else ""


class Bench:
    def __init__(self, a):
        self.a = a
        self.err_path = os.path.join(os.environ.get("TEMP", "."), "overlay-latency-err.txt")
        x, y, w, h = game_rect()
        self.box = (x, y, x + w, y + h)
        self.origin = (x, y, w, h)

    def start(self):
        subprocess.run(["taskkill", "/F", "/IM", "ba-key-AMAS.exe"], capture_output=True)
        time.sleep(1)
        env = dict(os.environ, BAOCR_MODELS=self.a.models, BAOCR_DEBUG_ASSIST="1")
        env.pop("BAOCR_PROBE", None)
        for k, v in (kv.split("=", 1) for kv in self.a.env):
            env[k] = v
        self.proc = subprocess.Popen([self.a.exe, "--assist", "--wiki", self.a.wiki], cwd=os.path.dirname(self.a.exe), env=env,
                                     stdout=subprocess.DEVNULL, stderr=open(self.err_path, "wb"))
        t_end = time.time() + 90
        while time.time() < t_end and not any("QuickFormation" in l for l in self.assist_lines()):
            time.sleep(0.2)
        if not any("QuickFormation" in l for l in self.assist_lines()):
            raise SystemExit("the add-on did not read 빠른 편성 (is the game on that screen, are the models found?)")
        time.sleep(4)

    def assist_lines(self):
        with open(self.err_path, "rb") as f:
            return [l for l in f.read().decode("utf-8", "replace").splitlines() if l.startswith("assist:")]

    def act(self, what):
        """Does the input, returns (change, settled, shown) in seconds after it, or None for shown when nothing was shown."""
        x0, y0, w, h = self.origin
        before = self.assist_lines()
        key = squad_of if what in ("remove", "add") else cards_of
        was = key(before[-1]) if before else ""
        n_before = len(before)
        samples = []
        stop = threading.Event()
        t_input = [None]

        def watch():
            prev = look(self.box)
            while not stop.is_set():
                time.sleep(0.02)
                cur = look(self.box)
                samples.append((time.time(), dist(prev, cur)))
                prev = cur
        th = threading.Thread(target=watch, daemon=True)
        th.start()
        time.sleep(0.15)
        if what in ("remove", "add"):
            u.SetCursorPos(x0 + int(600 * w / 800), y0 + int(238 * h / 600))
            time.sleep(0.1)
            t_input[0] = time.time()
            u.mouse_event(2, 0, 0, 0, 0)
            time.sleep(0.05)
            u.mouse_event(4, 0, 0, 0, 0)
        else:
            u.SetCursorPos(x0 + int(550 * w / 800), y0 + int(300 * h / 600))
            time.sleep(0.1)
            t_input[0] = time.time()
            for _ in range(2):
                u.mouse_event(0x0800, 0, 0, (-120 if what == "down" else 120) & 0xFFFFFFFF, 0)
                time.sleep(0.012)
        shown = None
        while time.time() - t_input[0] < 8.0:
            now = self.assist_lines()
            if len(now) > n_before and key(now[-1]) != was:
                shown = time.time() - t_input[0]
                break
            time.sleep(0.005)
        time.sleep(0.6)
        stop.set()
        th.join()
        t0 = t_input[0]
        moving = [t - t0 for t, d in samples if t > t0 and d > 1.0]
        change = moving[0] if moving else None
        settled = None
        if moving:
            last = moving[-1]
            settled = last
        return change, settled, shown

    def run(self):
        self.start()
        results = {}
        for pair in (("remove", "add"), ("down", "up")):
            for what in pair * 1:
                results[what] = []
        errors = acts = 0
        self.act("remove")
        self.act("add")  # warm-up of the pair
        time.sleep(2.0)
        for _ in range(self.a.trials):
            for what in ("remove", "add"):
                acts += 1
                r = self.act(what)
                if r[2] is None:
                    errors += 1
                results[what].append(r)
                time.sleep(2.0)
        self.act("down")
        self.act("up")
        time.sleep(2.0)
        for _ in range(self.a.trials):
            for what in ("down", "up"):
                acts += 1
                r = self.act(what)
                if r[2] is None:
                    errors += 1
                results[what].append(r)
                time.sleep(2.0)
        self.proc.terminate()
        return results, errors, acts


def summary(rows):
    shown = [r[2] for r in rows if r[2] is not None]
    over = [r[2] - r[1] for r in rows if r[2] is not None and r[1] is not None]
    chg = [r[0] for r in rows if r[0] is not None]
    st = [r[1] for r in rows if r[1] is not None]
    f = lambda v: (round(statistics.median(v) * 1000), round(max(v) * 1000)) if v else (None, None)
    return {"n": len(rows), "shown_median_max_ms": f(shown), "game_change_ms": f(chg), "game_settled_ms": f(st), "addon_after_settled_ms": f(over)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("exe")
    ap.add_argument("--trials", type=int, default=6)
    ap.add_argument("--models", default=r"D:\work-github\bluearchive-ocr\models")
    ap.add_argument("--wiki", default=r"D:\work-github\bluearchive-ocr\wikidb\wiki.db")
    ap.add_argument("--env", action="append", default=[], help="KEY=VALUE for the add-on")
    ap.add_argument("--out")
    a = ap.parse_args()
    results, errors, acts = Bench(a).run()
    out = {k: summary(v) for k, v in results.items()}
    out["errors"] = errors
    out["actions"] = acts
    for k in ("remove", "add", "down", "up"):
        print(f"{k:7s} {out[k]}")
    print(f"errors {errors} of {acts} actions")
    if a.out:
        with open(a.out, "w", encoding="utf-8") as f:
            json.dump(out, f, ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()
