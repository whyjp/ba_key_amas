"""Tokenises every student name (five bracket styles for names with a bracket) into the ids TrOCR is scored on.

    cargo run -q -p baocr-ocr --example dump_names > names.txt          (in scanner/)
    python scripts/trocr_seed.py names.txt path/to/trocr/model/tokenizer.json

Writes scanner/crates/ocr/data/trocr_surfaces.json: {name: [[id, ...], ...]}, one id list per written form
(`tokenizer.encode(form, add_special_tokens=False)`). Run it when the student list changes; the build never does.
"""
import json
import sys
from pathlib import Path

from tokenizers import Tokenizer

STYLES = [("(", ")"), ("[", "]"), ("《", "》"), ("〈", "〉"), ("【", "】")]


def forms(name):
    if "(" not in name:
        return [name]
    return [name.replace("(", a).replace(")", b) for a, b in STYLES]


def main(names_file, tokenizer_file):
    tok = Tokenizer.from_file(tokenizer_file)
    names = [n for n in Path(names_file).read_text(encoding="utf-8").split("\n") if n.strip()]
    out = {n: [tok.encode(f, add_special_tokens=False).ids for f in forms(n)] for n in names}
    dest = Path(__file__).resolve().parent.parent / "scanner/crates/ocr/data/trocr_surfaces.json"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(json.dumps(out, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8", newline="\n")
    print(len(out), "names,", sum(len(v) for v in out.values()), "forms ->", dest)


if __name__ == "__main__":
    main(*sys.argv[1:3])
