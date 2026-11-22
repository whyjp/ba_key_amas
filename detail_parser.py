"""Position-aware extraction from the Korean Blue Archive basic-info page.

Calibrated to a 1280x960 client. Every field is evidence-driven; unrecognized
values remain None. This complements label-based OCR for alternate layouts.
"""
from __future__ import annotations

import re
from PIL import Image


def _number(text: str, upper: int) -> int | None:
    m = re.search(r"(?:Lv[. ]*|레벨\s*)?(\d{1,3})", text, re.I)
    value = int(m.group(1)) if m else None
    return value if value is not None and 1 <= value <= upper else None


def _gold(r: int, g: int, b: int) -> bool:
    return r > 140 and g > 100 and b < 140 and r > g


def _cyan(r: int, g: int, b: int) -> bool:
    return g > 125 and b > 140 and r < 180 and b > r + 20


def _slot_count(image: Image.Image, x_values: list[int], y0: int, y1: int, predicate) -> int | None:
    rgb = image.convert("RGB")
    count = 0
    for x in x_values:
        pixels = sum(1 for xx in range(x-5, x+6) for yy in range(y0,y1)
                     if predicate(*rgb.getpixel((xx,yy))))
        if pixels >= 24:
            count += 1
    return count or None


def parse_basic_info(student, image: Image.Image, ocr):
    if image.size != (1280,960):
        image = image.resize((1280,960))
    words = ocr.read(image)
    def at(x0,y0,x1,y1):
        return sorted((w for w in words if x0 <= w.x <= x1 and y0 <= w.y <= y1), key=lambda w:w.x)
    student.raw_detail_ocr = " ".join(w.text for w in words)
    student.bond_rank = _number(" ".join(w.text for w in at(30,790,73,834)),100)
    if student.bond_rank is None:
        bond_crop = image.crop((30,790,82,838))
        student.bond_rank = _number(" ".join(w.text for w in ocr.read(bond_crop)),100)
    for field_name, x0, x1 in (
        ("ex_skill_level",690,789),("normal_skill_level",796,895),
        ("passive_skill_level",901,1000),("sub_skill_level",1007,1100)):
        setattr(student,field_name,_number(" ".join(w.text for w in at(x0,515,x1,550)),20))
    student.unique_weapon_level = _number(" ".join(w.text for w in at(765,562,875,608)),100)
    rarity = _slot_count(image,[270,285,300,315,330],806,831,_gold)
    student.rarity_stars = rarity if rarity and 1<=rarity<=5 else None
    # The bottom-right cyan weapon star(s), if any. Do not mistake equipment
    # icons or the main student rarity for weapon rank.
    rgb = image.convert('RGB')
    columns = [x for x in range(1000,1096) if sum(_cyan(*rgb.getpixel((x,y))) for y in range(627,650)) >= 3]
    groups = []
    for x in columns:
        if not groups or x > groups[-1][-1] + 2:
            groups.append([x])
        else:
            groups[-1].append(x)
    candidate = [g for g in groups if len(g)>=9]
    student.unique_weapon_stars = len(candidate) if 1<=len(candidate)<=5 else None
    roles = [w.text.upper() for w in at(330,800,450,838)]
    student.combat_role = next((v for v in roles if v in {'STRIKER','SPECIAL'}),None)
    student.status = "detail_ocr"
    return student
