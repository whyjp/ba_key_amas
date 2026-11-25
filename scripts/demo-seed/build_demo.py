"""Builds the demo accounts as upload bodies, one JSON file per teacher.

    python scripts/demo-seed/build_demo.py <out-dir>

Everything is derived from wikidb/wiki.db (the global students and their base stats) and personas.py, with a
fixed random seed per account, so the output is the same on every run. Nothing here reads a real user's scan.
The numbers stay inside what the game allows and what the server accepts (web/src/validate.ts). The rules below
that are about the game itself (skill gates, equipment tiers, stat growth) are approximations calibrated on one
real account of level 69, and are labelled so.
"""
import hashlib
import json
import math
import pathlib
import random
import re
import sqlite3
import sys

sys.path.insert(0, str(pathlib.Path(__file__).parent))
from personas import PERSONAS, TOP10, WIDELY_LOVED, random_personas  # noqa: E402

ROOT = pathlib.Path(__file__).resolve().parents[2]
DOMAIN = b"ba-key-amas/account/v1\0"
STAR_COEF = {1: 1.0, 2: 1.05, 3: 1.12, 4: 1.21, 5: 1.35}   # approximation of the star growth
UW_CAP = {1: 30, 2: 40, 3: 50, 4: 60}
TIER_LEVEL_CAP = [10, 20, 30, 40, 45, 50, 55, 60, 65, 70]  # same table as web/src/validate.ts
SKILL_GATES = [5, 10, 15, 25, 35, 45, 55, 65, 75]          # student level needed for skill level 2..10 (approximation)
EX_GATES = [10, 25, 40, 60]                                # for EX skill level 2..5 (approximation)

REGIONS = ["D.U. 시라토리구", "게헨나 중앙구", "밀레니엄 학습관", "백귀야행 중심부", "붉은겨울연방학원", "산해경중앙 특구",
           "살레생활관", "살레업무관", "아비도스본관", "트리니티광장구역", "하루하바라전자상가"]
# (volume, account level at which a typical player has cleared it)
STORY = [("0.총학생회", 65), ("1.대책위원회", 15), ("2.태엽감는꽃의파반느", 25), ("3.에덴조약", 35), ("4.카르바노그의토끼", 45),
         ("5.백화요란", 52), ("6.흘러간시간들의오라토리오", 58), ("Final.그리고 모든기적이 시작되는 곳", 62), ("Ex.데카그라마톤", 50),
         ("Ex.로어추적", 58), ("1.불꽃과그림자", 74), ("2.오디세이아", 80)]
EVENT_COUNT = 32
STYLE_SPEND = dict(newcomer=0.4, middle=0.55, story=0.5, school=0.6, collector=0.5, fan=0.7, role=0.6, outfit=0.85, veteran=0.9, bond=0.7, heavy=0.97, unlucky=0.12, lucky=0.97, random=0.5)


def normalise(code: str) -> str:
    s = re.sub(r"[^A-Z0-9]", "", code.upper())
    return s.replace("O", "0").replace("I", "1")


def code_of(index: int) -> str:
    return normalise(f"DEM0{index:09d}")


def account_id(code: str) -> str:
    return hashlib.sha256(DOMAIN + normalise(code).encode()).hexdigest()


def base_name(name: str) -> str:
    return re.split(r"[(*]", name)[0]


class Pool:
    def __init__(self, db: pathlib.Path):
        c = sqlite3.connect(f"file:{db.as_posix()}?mode=ro", uri=True)
        self.rows = []
        for sid, name, school, star, squad, role, pos, weapon, raw in c.execute(
            "select id,name_ko,school,star_grade,squad_type,tactic_role,position,weapon_type,raw from student where released_global=1 order by id"
        ):
            r = json.loads(raw)
            if any(x["name"] == name for x in self.rows):
                continue  # one name, two modes (Hoshino armed): the scan lists the name once, the first mode stays
            self.rows.append(dict(id=sid, name=name, base=base_name(name), school=school, star=star, squad=squad, role=role,
                                  position=(pos or "Front").upper(), weapon=weapon or r.get("WeaponType"), limited=bool(r.get("IsLimited")),
                                  equipment=r.get("Equipment") or [], hp=(r["MaxHP1"], r["MaxHP100"]), atk=(r["AttackPower1"], r["AttackPower100"]),
                                  dfn=(r["DefensePower1"], r["DefensePower100"]), heal=(r["HealPower1"], r["HealPower100"])))
        self.bases = {r["base"] for r in self.rows}


def stat(pair, level, star):
    lo, hi = pair
    return int((lo + (hi - lo) * (level - 1) / 99) * STAR_COEF[star])


def weight(p, s, persona):
    w = 1.0
    # a taste only tilts the draw a little: students come from gacha, and loved ones are handled separately (spend)
    if s["base"] in TOP10:
        w *= 1.25
    elif s["base"] in WIDELY_LOVED:
        w *= 1.1
    if s["school"] in persona.get("schools", []):
        w *= 2
    if s["role"] in persona.get("roles", []):
        w *= 2
    if "(" in s["name"] or "*" in s["name"]:
        w *= persona.get("outfits", 1.0)
    if s["star"] <= 2:
        w *= 1.5
    if s["limited"] and persona.get("outfits", 1.0) < 3:
        w *= 0.5
    return w


def sample(rng, items, weights, k):
    keyed = sorted(((rng.random() ** (1.0 / max(w, 1e-9)), i) for i, w in zip(items, weights)), reverse=True)
    return [i for _, i in keyed[:k]]


def skill_max(level, gates):
    return 1 + sum(1 for g in gates if level >= g)


def build_student(rng, s, level, star, bond, pinned, invest, account_level):
    ex = max(1, round(invest * skill_max(level, EX_GATES) + rng.uniform(-0.4, 0.4))) if level > 1 else 1
    ex = min(ex, skill_max(level, EX_GATES), 5)

    def other():
        top = skill_max(level, SKILL_GATES)
        return max(1, min(top, round(invest * top + rng.uniform(-1.2, 1.2)))) if level > 1 else 1

    out = dict(name=s["name"], level=level, rarity_stars=star, ex_skill_level=ex, normal_skill_level=other(), passive_skill_level=other(),
               sub_skill_level=other(), bond_rank=bond, combat_role="STRIKER" if s["squad"] == "Main" else "SPECIAL", is_pinned=pinned,
               reference={"id": s["id"]})
    if star == 5 and s["star"] == 3 and level >= 30 and rng.random() < 0.15 + 0.85 * invest:
        stars = 1 + (rng.random() < invest * 0.8) + (rng.random() < invest * 0.5) + (rng.random() < invest * 0.2)
        stars = min(stars, 4 if invest >= 0.8 else 3)
        cap = min(UW_CAP[stars], level)
        out["unique_weapon_stars"] = stars
        out["unique_weapon_level"] = max(1, cap - (rng.randint(0, 6) if invest > 0.5 else rng.randint(0, 25)))
    # a real level 69 account shows stats above the bare formula (gear and bond add to them): median ratio real/formula
    # over 29 students at level 40 or more was hp 1.11, attack 1.37, defense 0.89, healing 1.07. The same ratios, scaled by how
    # far the student is invested, are applied here.
    gain = (0.4 + 0.6 * invest) if level > 1 else 0.0
    out["stats"] = {key: int(stat(s[src], level, star) * (1 + (ratio - 1) * gain))
                    for key, src, ratio in (("max_hp", "hp", 1.11), ("attack", "atk", 1.37), ("defense", "dfn", 0.89), ("healing", "heal", 1.07))}
    out["loadout"] = dict(position=s["position"], weapon_class=s["weapon"] if s["weapon"] and re.fullmatch(r"[A-Z]{2,3}", s["weapon"]) else None)
    if out["loadout"]["weapon_class"] is None:
        del out["loadout"]["weapon_class"]
    max_tier = max(1, min(9, 1 + (account_level - 14) // 10))
    equipment = []
    for slot, kind in enumerate(s["equipment"][:3], start=1):
        unlock = (1, 15, 35)[slot - 1]
        if level < unlock:
            equipment.append(dict(slot=slot, slot_kind="standard", equipped=False))
            continue
        tier = max(1, min(max_tier, round(invest * max_tier + rng.uniform(-1, 1)) if level > 1 else 1))
        cap = TIER_LEVEL_CAP[tier - 1]
        item_level = max(1, cap - rng.choice([0, 0, 0, 5, 10]) if level > 1 else 1)
        equipment.append(dict(slot=slot, slot_kind="standard", equipped=True, type=kind, tier=tier, level=min(item_level, cap)))
    if equipment:
        out["equipment"] = equipment
    return out


def build_account(pool, persona, index):
    code = code_of(index)
    rng = random.Random(int(account_id(code)[:12], 16))
    lv = persona["level"]
    rows = pool.rows
    # Gacha: a loved student was pulled with probability `spend` (outfits are harder than the base student, limited ones
    # harder still), so even a devoted account misses some of them. The rest of the roster is a weighted draw.
    spend = min(1.0, max(0.05, persona.get("spend", STYLE_SPEND[persona["style"]]) + rng.gauss(0, 0.12)))
    luck = persona.get("luck", rng.random() * 0.6)
    chosen = []
    for s in rows:
        if s["base"] in persona["love"]:
            p = spend * (0.55 if ("(" in s["name"] or "*" in s["name"]) else 0.85) * (0.7 if s["limited"] else 1.0)
            if rng.random() < p:
                chosen.append(s)
    target = min(persona["owned"], len(rows))
    if len(chosen) > target:
        chosen = rng.sample(chosen, target)
    left = [s for s in rows if s not in chosen]
    owned = chosen + sample(rng, left, [weight(pool, s, persona) for s in left], target - len(chosen))
    loved = sorted((s for s in owned if s["base"] in persona["love"]), key=lambda s: (persona["love"].index(s["base"]), -s["star"], s["id"]))
    # only so many students get the deep bond: the rest of a loved character's outfits are owned and trained, not adored
    favourites = loved[: len(persona["bond_top"]) + 2]
    rest = [s for s in owned if s not in favourites]
    # the favourites are trained first, then the others by the same weights, strikers first
    train_w = [weight(pool, s, persona) * (2.0 if s["squad"] == "Main" else 1.0) for s in rest]
    trained = (favourites + sample(rng, rest, train_w, max(0, persona["trained"] - len(favourites))))[: persona["trained"]]
    trained_ids = {s["id"] for s in trained}
    bond_top = persona["bond_top"]
    fav_bond = {}
    for i, s in enumerate(favourites):
        top = bond_top[i] if i < len(bond_top) else max(2, int(bond_top[-1] * rng.uniform(0.5, 0.85)))
        fav_bond[s["id"]] = top if (i == 0 and top == 100) else max(2, int(top * rng.uniform(0.7, 1.0)))
    pin_limit = rng.randint(min(3, len(favourites)), min(15, max(3, len(favourites))))
    pinned = {s["id"] for s in favourites[:pin_limit]}
    entries = []
    for s in sorted(owned, key=lambda s: (s["id"] not in trained_ids, trained.index(s) if s in trained else 0, s["id"])):
        is_fav = s["id"] in fav_bond
        if s["id"] in trained_ids:
            level = lv if is_fav and rng.random() < 0.7 else max(10, lv - int(rng.expovariate(1.0 / max(3.0, 0.14 * lv))))
            level = min(level, lv)
        else:
            level = 1 if rng.random() < 0.88 else rng.randint(2, 12)
        extra = 0
        if is_fav:
            extra = 5 if rng.random() < 0.1 + 0.5 * spend + 0.2 * luck else rng.randint(0, 3)
        elif s["id"] in trained_ids and rng.random() < 0.1 + luck * 0.5:
            extra = rng.choice([1, 1, 1, 2, 2, 3, 5])   # duplicates come in by chance, not by taste
        elif rng.random() < 0.04 + luck * 0.12:
            extra = rng.choice([1, 1, 2])
        star = min(5, s["star"] + extra)
        if is_fav:
            bond = fav_bond[s["id"]]
        elif s["id"] in trained_ids:
            bond = min(max(2, bond_top[-1] - 1), 2 + int(rng.expovariate(1.0 / max(2.0, lv / 12))))
        else:
            bond = 1 + int(rng.expovariate(1.0 / 1.2))
            bond = min(bond, 9)
        invest = min(1.0, persona["invest"] + (0.15 if is_fav else 0.0)) * (1.0 if s["id"] in trained_ids else 0.2)
        entries.append(dict(student=build_student(rng, s, level, star, bond, s["id"] in pinned, invest, lv)))
    names = [e["student"]["name"] for e in entries]
    bonds = {e["student"]["name"]: e["student"]["bond_rank"] for e in entries}

    def memorial(name):
        b = bonds[name]
        return b >= 9 or (b >= 5 and rng.random() < 0.5) or (b >= 2 and rng.random() < 0.15)

    duty = [dict(name=n, selected=False, memorial_lobby=memorial(n)) for n in names]
    for d in rng.sample(duty, min(len(duty), min(10, 3 + lv // 12))):
        d["selected"] = True
    unread_pool = rng.sample(names, min(len(names), rng.randint(3, 13)))
    unread = [dict(name=n, count=rng.randint(1, 2)) for n in unread_pool]
    base_rank = (lv - 10) / 12 + 1
    regions = [dict(name=r, rank=max(1, min(9, round(base_rank + rng.uniform(-0.9, 0.9))))) for r in REGIONS]
    p_event = min(0.97, max(0.05, (lv - 15) / 50))
    progress = dict(
        schema_version=2, taken_at="2026-11-24T21:00:00+09:00", layout="4:3",
        account=dict(nickname=persona["nick"], level=lv, circle=None),
        schedule=dict(regions=regions, total_rank=sum(r["rank"] for r in regions)),
        missions=dict(areas=[]),
        story=dict(main=[dict(volume=v, chapters=[], cleared=(True if lv >= need else (False if lv >= need - 8 else None))) for v, need in STORY], mini=[], group=[]),
        events=dict(open=None, past=[dict(index=i, cleared=rng.random() < p_event, scanned_while_open=False) for i in range(EVENT_COUNT)]),
        momotalk=dict(students=[], unread_total=sum(u["count"] for u in unread), unread=unread),
        duty=dict(selected_count=sum(d["selected"] for d in duty), limit=10, students=duty), warnings=[])
    cap = round(80 + 1.73 * (lv - 1))
    roster = dict(schema_version=3, entries=entries, account=dict(credits=int(rng.uniform(0.3, 1.6) * lv ** 2.4 * 5000), pyroxene=rng.randint(300, 3000 + lv * 110),
                                                ap=f"{rng.randint(0, cap * 2)}/{cap}"))
    return dict(account_id=account_id(code), nickname=persona["nick"], public=True, roster=roster, progress=progress), code


def main():
    out = pathlib.Path(sys.argv[1])
    out.mkdir(parents=True, exist_ok=True)
    pool = Pool(ROOT / "wikidb" / "wiki.db")
    personas = PERSONAS + random_personas(pool.bases)
    missing = {b for p in PERSONAS for b in p["love"]} - pool.bases
    if missing:
        raise SystemExit(f"names not in the global roster: {sorted(missing)}")
    for i, persona in enumerate(personas, start=1):
        body, _ = build_account(pool, persona, i)
        (out / f"demo-{i:02d}.json").write_text(json.dumps(body, ensure_ascii=False, separators=(",", ":")), encoding="utf-8", newline="\n")
        print(f"demo-{i:02d} lv{persona['level']:>2} students={len(body['roster']['entries']):>3} {persona['nick']}")


if __name__ == "__main__":
    main()
