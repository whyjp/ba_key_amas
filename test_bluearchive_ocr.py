from pathlib import Path
import json
from PIL import Image
from bluearchive_ocr import Student, Word, parse_detail_text, parse_roster_tile, save_results, text_lines


def test_roster_tile():
    s = parse_roster_tile([Word("사츠기(수영복)", 1, 1)], [Word("LⅥ68", 1, 1)])
    assert s.name == "사츠기(수영복)" and s.level == 68


def test_missing_data_never_invented():
    s = parse_detail_text(Student("미카", level=68), "인연 랭크 12\nEX 스킬 Lv. 5\n기본 스킬 9\n강화 스킬 8\n서브 스킬 7\n전용 무기 Lv. 50\n성급 5")
    assert (s.bond_rank, s.ex_skill_level, s.normal_skill_level, s.passive_skill_level, s.sub_skill_level) == (12, 5, 9, 8, 7)
    assert s.unique_weapon_level == 50 and s.rarity_stars == 5
    assert s.unique_weapon_stars is None


def test_export_xss_safe(tmp_path: Path):
    save_results([Student("<script>alert(1)</script>")], tmp_path, source="test")
    document = (tmp_path / "students.html").read_text(encoding="utf-8")
    assert "<script>alert(1)</script>" not in document
    assert json.loads((tmp_path / "students.json").read_text(encoding="utf-8"))["count"] == 1
    assert (tmp_path / "students.csv").read_bytes().startswith(b"\xef\xbb\xbf")


def test_text_lines():
    assert text_lines([Word("5", 100, 12), Word("EX", 0, 10), Word("인연", 0, 50)]) == "EX 5\n인연"
