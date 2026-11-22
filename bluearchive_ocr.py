"""Blue Archive visual roster OCR prototype (Windows, Korean UI).

No memory reading, packet interception, or game-file access. OCR failure is
represented as null, never inferred from character identity.
"""
from __future__ import annotations

import argparse
import asyncio
import csv
from dataclasses import asdict, dataclass, field
from datetime import datetime, timezone
import html
import json
from pathlib import Path
import re
import sys
import time
from typing import Protocol

from PIL import Image

BASE_W, BASE_H = 1280, 960  # game client, excluding 30px window title bar
COL_CENTERS = [140, 336, 532, 728, 924, 1120]
ROW_TOPS = [196, 466, 736]
NAME_PATTERN = re.compile(r"[가-힣A-Za-z][가-힣A-Za-z0-9\s·*☆★'()\-]*")
LEVEL_PATTERN = re.compile(r"(?:Lv\.?|L[vⅥ]|레벨|Level)?\s*[.:]?\s*(\d{1,3})", re.I)
SKILL_LABELS = {
    "ex_skill_level": r"(?:EX|ＥＸ)\s*(?:스킬|SKILL)?",
    "normal_skill_level": r"(?:기본|노말|NORMAL)\s*(?:스킬|SKILL)?",
    "passive_skill_level": r"(?:강화|패시브|PASSIVE)\s*(?:스킬|SKILL)?",
    "sub_skill_level": r"(?:서브|SUB)\s*(?:스킬|SKILL)?",
}


@dataclass
class Student:
    name: str
    level: int | None = None
    rarity_stars: int | None = None
    ex_skill_level: int | None = None
    normal_skill_level: int | None = None
    passive_skill_level: int | None = None
    sub_skill_level: int | None = None
    bond_rank: int | None = None
    unique_weapon_stars: int | None = None
    unique_weapon_level: int | None = None
    combat_role: str | None = None
    raw_detail_ocr: str | None = None
    evidence_screenshot: str | None = None
    status: str = "roster_only"
    warnings: list[str] = field(default_factory=list)


@dataclass
class Word:
    text: str
    x: float
    y: float


class Ocr(Protocol):
    def read(self, image: Image.Image) -> list[Word]: ...


class WindowsOcr:
    """Windows built-in Korean OCR via WinRT; no model download/API key."""
    def __init__(self):
        from winrt.windows.globalization import Language
        from winrt.windows.media.ocr import OcrEngine
        self.engine = OcrEngine.try_create_from_language(Language("ko-KR"))
        if self.engine is None:
            raise RuntimeError("Windows Korean OCR language is unavailable; install Korean OCR language features")

    async def _read(self, image: Image.Image) -> list[Word]:
        from winrt.windows.graphics.imaging import SoftwareBitmap, BitmapPixelFormat, BitmapAlphaMode
        from winrt.windows.storage.streams import Buffer
        image = image.convert("RGBA")
        # Upscaling improves recognition of the compact roster captions.
        scale = 2 if image.width < 1600 else 1
        if scale != 1:
            image = image.resize((image.width * scale, image.height * scale))
        raw = image.tobytes("raw", "BGRA")
        buffer = Buffer(len(raw))
        buffer.length = len(raw)
        memoryview(buffer)[:] = raw
        bitmap = SoftwareBitmap(BitmapPixelFormat.BGRA8, image.width, image.height, BitmapAlphaMode.IGNORE)
        bitmap.copy_from_buffer(buffer)
        result = await self.engine.recognize_async(bitmap)
        return [Word(w.text, w.bounding_rect.x / scale, w.bounding_rect.y / scale)
                for line in result.lines for w in line.words]

    def read(self, image: Image.Image) -> list[Word]:
        return asyncio.run(self._read(image))


def crop_scaled(image: Image.Image, rect: tuple[int, int, int, int]) -> Image.Image:
    sx, sy = image.width / BASE_W, image.height / BASE_H
    return image.crop(tuple(round(v * (sx if i % 2 == 0 else sy)) for i, v in enumerate(rect)))


def normalize_name(text: str) -> str:
    text = re.sub(r"\s+", "", text).replace("（", "(").replace("）", ")")
    return text.replace("＊", "*").strip("·.-_ ")


def parse_roster_tile(name_words: list[Word], level_words: list[Word]) -> Student | None:
    name = normalize_name("".join(w.text for w in sorted(name_words, key=lambda w: w.x)))
    if not NAME_PATTERN.fullmatch(name) or not (1 <= len(name) <= 35):
        return None
    raw_level = "".join(w.text for w in sorted(level_words, key=lambda w: w.x))
    match = LEVEL_PATTERN.search(raw_level.replace("O", "0"))
    level = int(match.group(1)) if match else None
    if level is not None and not (1 <= level <= 150):
        level = None
    return Student(name=name, level=level, warnings=[] if level else [f"level OCR uncertain: {raw_level!r}"])


def _edit_distance(a: str, b: str) -> int:
    prev = list(range(len(b) + 1))
    for i, ca in enumerate(a, 1):
        curr = [i]
        for j, cb in enumerate(b, 1):
            curr.append(min(curr[-1] + 1, prev[j] + 1, prev[j-1] + (ca != cb)))
        prev = curr
    return prev[-1]


def parse_roster(image: Image.Image, ocr: Ocr) -> list[tuple[Student, tuple[int, int]]]:
    """Return only full card rows; a partially visible last row is ignored."""
    found: list[tuple[Student, tuple[int, int]]] = []
    # A wider OCR context sometimes disambiguates characters ('키' vs '카').
    full_words = ocr.read(image)
    for top in ROW_TOPS:
        if (top + 245) * image.height / BASE_H > image.height - 12:
            continue
        for x in COL_CENTERS:
            # Three small crops instead of full-screen OCR: less portrait noise.
            names = ocr.read(crop_scaled(image, (x - 93, top + 207, x + 93, top + 250)))
            levels = ocr.read(crop_scaled(image, (x - 78, top + 152, x + 78, top + 210)))
            student = parse_roster_tile(names, levels)
            if student:
                context_words = [w for w in full_words if x - 93 <= w.x <= x + 93
                                 and top + 207 <= w.y <= top + 250]
                context_name = normalize_name("".join(w.text for w in sorted(context_words, key=lambda w: w.x)))
                if (NAME_PATTERN.fullmatch(context_name)
                        and _edit_distance(context_name, student.name) <= 2
                        and context_name != student.name):
                    student.warnings.append(f"Name OCR corrected from tile {student.name!r} by full-image context")
                    student.name = context_name
                found.append((student, (x, top + 95)))
    return found


def _near_number(lines: list[str], label: str, limit: int) -> int | None:
    for i, line in enumerate(lines):
        match = re.search(label, line, re.I)
        if match:
            # Prefer the same line; inspect next line only for label-only rows.
            tail = line[match.end():]
            if not tail.strip() and i + 1 < len(lines):
                tail = lines[i + 1]
            number = re.search(r"(?:Lv\.?|레벨|Level)?\s*[.:]?\s*(\d{1,3})", tail, re.I)
            if number and 1 <= int(number.group(1)) <= limit:
                return int(number.group(1))
    return None


def parse_detail_text(student: Student, text: str) -> Student:
    """Conservative label-based extraction; unknown stays null."""
    lines = [s.strip() for s in text.splitlines() if s.strip()]
    student.raw_detail_ocr = text
    student.bond_rank = _near_number(lines, r"인연\s*(?:랭크|레벨|Lv)?|Bond\s*(?:Rank|Lv)?", 100)
    for field_name, label in SKILL_LABELS.items():
        setattr(student, field_name, _near_number(lines, label, 20))
    student.unique_weapon_level = _near_number(lines, r"전용\s*무기\s*(?:Lv|레벨)?|Unique\s*Weapon\s*(?:Lv|Level)?", 100)
    # Only accept star count explicitly labeled; stars visually rendered as icons
    # are not text and require screen-specific ROI calibration.
    for field_name, label in (("rarity_stars", r"(?:성급|등급|Rarity)"),
                              ("unique_weapon_stars", r"(?:전용\s*무기\s*(?:성급|등급)|Unique\s*Weapon\s*Stars)")):
        match = re.search(label + r"\s*[:：]?\s*([1-5])\s*(?:성|[★☆]?)", text, re.I)
        if match:
            setattr(student, field_name, int(match.group(1)))
    student.status = "detail_ocr"
    return student


def save_results(students: list[Student], output: Path, *, source: str) -> None:
    output.mkdir(parents=True, exist_ok=True)
    payload = {"schema_version": 1, "captured_at": datetime.now(timezone.utc).isoformat(),
               "source": source, "count": len(students), "students": [asdict(s) for s in students]}
    (output / "students.json").write_text(json.dumps(payload, ensure_ascii=False, indent=2), encoding="utf-8")
    columns = list(Student.__dataclass_fields__)
    with (output / "students.csv").open("w", encoding="utf-8-sig", newline="") as fp:
        writer = csv.DictWriter(fp, fieldnames=columns)
        writer.writeheader()
        for student in students:
            row = asdict(student)
            row["warnings"] = " | ".join(row["warnings"])
            writer.writerow(row)
    # Self-contained HTML; escape all OCR text to prevent injection.
    data = json.dumps(payload, ensure_ascii=False).replace("<", "\\u003c").replace(">", "\\u003e").replace("&", "\\u0026")
    viewer = f'''<!doctype html><html lang="ko"><meta charset="utf-8"><title>Blue Archive roster OCR</title>
<style>body{{font:14px system-ui;margin:24px;background:#eef6fb;color:#1c3144}}input{{padding:8px;width:300px}}table{{border-collapse:collapse;background:white;width:100%}}th,td{{padding:7px;border-bottom:1px solid #dde6ec;text-align:left;white-space:nowrap}}th{{position:sticky;top:0;background:#d8effb;cursor:pointer}}.wrap{{overflow:auto;max-height:80vh}}small{{color:#536779}}</style>
<h1>학생 OCR 데이터</h1><p><span id="count"></span> · 원본: <span id="source"></span> · 미인식 값은 빈 칸입니다.</p><input id="q" placeholder="학생 이름 검색"><div class="wrap"><table><thead id="headers"></thead><tbody id="rows"></tbody></table></div>
<script id="payload" type="application/json">{data}</script>
<script>
const p=JSON.parse(document.getElementById('payload').textContent), keys=['name','level','rarity_stars','ex_skill_level','normal_skill_level','passive_skill_level','sub_skill_level','bond_rank','unique_weapon_stars','unique_weapon_level','status','warnings'];
const labels=['학생','레벨','성급','EX','기본','강화','서브','인연 랭크','전용무기 성급','전용무기 레벨','상태','주의'];
let sort='name',desc=false; document.getElementById('source').textContent=p.source;
document.getElementById('headers').innerHTML='<tr>'+keys.map((k,i)=>'<th data-key="'+k+'">'+labels[i]+'</th>').join('')+'</tr>';
function render(){{const q=document.getElementById('q').value.trim().toLowerCase();let a=p.students.filter(s=>s.name.toLowerCase().includes(q));a.sort((x,y)=>{{let u=x[sort],v=y[sort];return (desc?-1:1)*((typeof u==='number'&&typeof v==='number')?u-v:String(u??'').localeCompare(String(v??''),'ko'));}});document.getElementById('count').textContent=a.length+' / '+p.students.length+'명';let body=document.getElementById('rows');body.replaceChildren();for(const s of a){{let tr=document.createElement('tr');for(const k of keys){{let td=document.createElement('td');td.textContent=Array.isArray(s[k])?s[k].join(' | '):(s[k]??'');tr.append(td);}}body.append(tr);}}}}
document.getElementById('q').oninput=render;document.getElementById('headers').onclick=e=>{{let k=e.target.dataset.key;if(!k)return;desc=sort===k?!desc:false;sort=k;render()}};render();
</script></html>'''
    (output / "students.html").write_text(viewer, encoding="utf-8")


def screenshot_client(hwnd: int) -> Image.Image:
    import mss
    import win32gui
    left, top = win32gui.ClientToScreen(hwnd, (0, 0))
    right, bottom = win32gui.GetClientRect(hwnd)[2:]
    with mss.mss() as sct:
        shot = sct.grab({"left": left, "top": top, "width": right, "height": bottom})
        return Image.frombytes("RGB", shot.size, shot.rgb)


def find_game() -> int:
    import win32gui
    handles: list[int] = []
    win32gui.EnumWindows(lambda h, _: handles.append(h) if win32gui.IsWindowVisible(h)
                         and "Blue Archive" in win32gui.GetWindowText(h) else None, None)
    if not handles:
        raise RuntimeError("Blue Archive window not found")
    return handles[0]


def is_roster(image: Image.Image, ocr: Ocr) -> bool:
    title = "".join(w.text for w in ocr.read(crop_scaled(image, (38, 75, 238, 145))))
    return "학생" in title and "리스트" in title


def perform_click(x: int, y: int, hwnd: int) -> None:
    """Foreground game input. NEVER called in --image mode."""
    import win32api
    import win32con
    import win32gui
    if win32gui.GetForegroundWindow() != hwnd:
        raise RuntimeError("Blue Archive lost foreground; stopping input")
    left, top = win32gui.ClientToScreen(hwnd, (0, 0))
    _, _, width, height = win32gui.GetClientRect(hwnd)
    win32api.SetCursorPos((left + round(x * width / BASE_W), top + round(y * height / BASE_H)))
    win32api.mouse_event(win32con.MOUSEEVENTF_LEFTDOWN, 0, 0)
    win32api.mouse_event(win32con.MOUSEEVENTF_LEFTUP, 0, 0)


def perform_scroll(hwnd: int, ticks: int = 3) -> None:
    import win32api
    import win32con
    import win32gui
    if win32gui.GetForegroundWindow() != hwnd:
        raise RuntimeError("Blue Archive lost foreground; stopping input")
    left, top = win32gui.ClientToScreen(hwnd, (0, 0))
    _, _, width, height = win32gui.GetClientRect(hwnd)
    win32api.SetCursorPos((left + round(1180 * width / BASE_W), top + round(750 * height / BASE_H)))
    win32api.mouse_event(win32con.MOUSEEVENTF_WHEEL, 0, 0, -120 * ticks)


def perform_escape(hwnd: int) -> None:
    import win32api
    import win32con
    import win32gui
    if win32gui.GetForegroundWindow() != hwnd:
        raise RuntimeError("Blue Archive lost foreground; stopping input")
    win32api.keybd_event(win32con.VK_ESCAPE, 0, 0, 0)
    win32api.keybd_event(win32con.VK_ESCAPE, 0, win32con.KEYEVENTF_KEYUP, 0)


def text_lines(words: list[Word]) -> str:
    """Group approximate OCR baselines while preserving left-to-right order."""
    rows: list[list[Word]] = []
    for word in sorted(words, key=lambda w: (w.y, w.x)):
        row = next((r for r in rows if abs(r[0].y - word.y) <= 10), None)
        if row is None:
            row = []
            rows.append(row)
        row.append(word)
    return "\n".join(" ".join(w.text for w in sorted(row, key=lambda w: w.x)) for row in rows)


def scan_live(ocr: Ocr, output: Path, max_pages: int, details: bool = True) -> list[Student]:
    import win32gui
    hwnd = find_game()
    if win32gui.GetForegroundWindow() != hwnd:
        raise RuntimeError("For safety, bring Blue Archive to foreground before --scan-live")
    image = screenshot_client(hwnd)
    if not is_roster(image, ocr):
        raise RuntimeError("Open 학생 리스트 / All first; refusing to navigate other menus")
    students: dict[str, Student] = {}
    stagnant = 0
    evidence = output / "screenshots"
    evidence.mkdir(parents=True, exist_ok=True)
    for page in range(max_pages):
        image = screenshot_client(hwnd)
        if not is_roster(image, ocr):
            raise RuntimeError("Student roster disappeared; stopped without further input")
        tiles = parse_roster(image, ocr)
        new = [(s, xy) for s, xy in tiles if s.name not in students]
        print(f"page {page+1}: {len(tiles)} visible, {len(new)} new", flush=True)
        for student, (x, y) in new:
            students[student.name] = student
            if details:
                perform_click(x, y, hwnd)
                time.sleep(1.1)
                detail = screenshot_client(hwnd)
                if is_roster(detail, ocr):
                    student.status = "detail_failed"
                    student.warnings.append("Click did not open detail; stopped to avoid uncontrolled actions")
                    save_results(list(students.values()), output, source="live_roster")
                    return list(students.values())
                filename = f"detail_{page+1:03}_{len(students):03}.png"
                detail.save(evidence / filename)
                student.evidence_screenshot = f"screenshots/{filename}"
                parse_detail_text(student, text_lines(ocr.read(detail)))
                save_results(list(students.values()), output, source="live_roster")
                perform_escape(hwnd)
                time.sleep(0.9)
                if not is_roster(screenshot_client(hwnd), ocr):
                    student.status = "return_failed"
                    student.warnings.append("Escape did not restore roster; stopped")
                    save_results(list(students.values()), output, source="live_roster")
                    return list(students.values())
        image.save(evidence / f"roster_{page+1:03}.png")
        save_results(list(students.values()), output, source="live_roster")
        stagnant = stagnant + 1 if not new else 0
        if stagnant >= 3:
            break
        if page == max_pages - 1:
            break
        perform_scroll(hwnd)
        time.sleep(1.2)
    return list(students.values())


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Blue Archive roster OCR: screenshot dry-run or explicit live scan")
    source = parser.add_mutually_exclusive_group(required=True)
    source.add_argument("--image", type=Path, help="Existing roster screenshot; no computer input")
    source.add_argument("--scan-live", action="store_true", help="Opt-in live foreground scroll capture")
    parser.add_argument("--output", type=Path, default=Path("output"))
    parser.add_argument("--max-pages", type=int, default=40)
    parser.add_argument("--roster-only", action="store_true", help="Do not click student detail cards")
    args = parser.parse_args(argv)
    try:
        ocr = WindowsOcr()
        if args.image:
            image = Image.open(args.image).convert("RGB")
            if image.height / image.width > 0.76:  # sample cua screenshot includes 30px title
                image = image.crop((0, 30, image.width, image.height))
            students = list({s.name: s for s, _ in parse_roster(image, ocr)}.values())
            save_results(students, args.output, source=str(args.image))
        else:
            if args.max_pages < 1:
                parser.error("--max-pages must be >= 1")
            students = scan_live(ocr, args.output, args.max_pages, details=not args.roster_only)
        print(f"Exported {len(students)} unique students to {args.output.resolve()}")
        return 0
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
