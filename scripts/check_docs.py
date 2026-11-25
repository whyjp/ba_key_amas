#!/usr/bin/env python3
"""Mechanical checks for the documentation. Run from anywhere:

    python -I scripts/check_docs.py

Checks:
  1. Every relative Markdown link in README.md and docs/**/*.md points to an existing file, and a
     `#anchor` names a heading of the target Markdown file.
  2. Every href and src in README.md and docs/*.html points to an existing file.
  3. docs/diagrams/crates.svg equals what scripts/gen_arch.py generates (it runs `cargo metadata`,
     which does not build anything; the output goes to a temporary folder).
  4. No Markdown doc outside docs/history contains an em dash, an en dash or a curly quote,
     except inside code fences.

Exit code 0 when everything passes, 1 with a list of problems otherwise. Standard library only.
"""
import importlib.util
import pathlib
import re
import sys
import tempfile
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent
DOCS = ROOT / "docs"

MD_LINK = re.compile(r"(?<!\!)\[[^\]\n]*\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)|!\[[^\]\n]*\]\(([^)\s]+)\)")
HTML_REF = re.compile(r"""\b(?:href|src)\s*=\s*["']([^"']+)["']""", re.I)
BAD_CHARS = re.compile("[–—‘’“”]")
FENCE = re.compile(r"^\s*(```|~~~)")
INLINE_CODE = re.compile(r"`[^`\n]*`")


def rel(p):
    return p.relative_to(ROOT).as_posix()


def md_files():
    files = [ROOT / "README.md"] + sorted(DOCS.rglob("*.md"))
    return [f for f in files if f.exists()]


def prose_lines(path):
    """Yield (line number, text) for lines outside code fences, with inline code blanked out."""
    in_fence = False
    for n, line in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        if FENCE.match(line):
            in_fence = not in_fence
            continue
        if not in_fence:
            yield n, INLINE_CODE.sub("``", line)


def slug(heading):
    """GitHub-style anchor for a heading line."""
    text = re.sub(r"`", "", heading.strip().lstrip("#").strip())
    text = re.sub(r"\[([^\]]*)\]\([^)]*\)", r"\1", text)
    text = text.lower()
    text = re.sub(r"[^\w\- ]", "", text)
    return text.replace(" ", "-")


def anchors(path, cache={}):
    if path not in cache:
        found, seen = set(), {}
        for _, line in prose_lines(path):
            if re.match(r"#{1,6}\s", line):
                s = slug(line)
                k = seen.get(s, 0)
                seen[s] = k + 1
                found.add(s if k == 0 else f"{s}-{k}")
        cache[path] = found
    return cache[path]


def is_external(target):
    return re.match(r"^[a-z][a-z0-9+.-]*:", target, re.I) is not None or target.startswith("//")


def check_target(source, line, target, problems, want_anchor):
    if not target or is_external(target):
        return
    path_part, _, anchor = target.partition("#")
    path_part = urllib.parse.unquote(path_part.split("?")[0])
    dest = source if not path_part else (source.parent / path_part).resolve()
    if not dest.exists():
        problems.append(f"{rel(source)}:{line}: link target not found: {target}")
        return
    if want_anchor and anchor and dest.suffix == ".md" and dest.is_file():
        if urllib.parse.unquote(anchor).lower() not in anchors(dest):
            problems.append(f"{rel(source)}:{line}: no heading for anchor in {target}")


def check_markdown_links(problems):
    for f in md_files():
        for n, line in prose_lines(f):
            for m in MD_LINK.finditer(line):
                check_target(f, n, m.group(1) or m.group(2), problems, True)


def check_html_refs(problems):
    for f in sorted(DOCS.glob("*.html")):
        text = f.read_text(encoding="utf-8")
        for m in HTML_REF.finditer(text):
            line = text.count("\n", 0, m.start()) + 1
            check_target(f, line, m.group(1), problems, False)


def check_diagram(problems):
    spec = importlib.util.spec_from_file_location("gen_arch", ROOT / "scripts" / "gen_arch.py")
    gen = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(gen)
    with tempfile.TemporaryDirectory() as tmp:
        gen.DOCS = pathlib.Path(tmp)  # main() writes under DOCS, so nothing in the repository changes
        try:
            gen.main()
        except Exception as e:  # cargo missing, metadata failure
            problems.append(f"docs/diagrams/crates.svg: scripts/gen_arch.py failed: {e}")
            return
        fresh = (pathlib.Path(tmp) / "diagrams" / "crates.svg").read_bytes()
    current = (DOCS / "diagrams" / "crates.svg").read_bytes()
    # the file is written with the line endings of the platform that made it (CRLF on Windows, LF on the CI runner):
    # the picture is the same, so the endings are not compared
    if fresh.replace(b"\r\n", b"\n") != current.replace(b"\r\n", b"\n"):
        problems.append("docs/diagrams/crates.svg is out of date: run `python -I scripts/gen_arch.py`")


def check_characters(problems):
    history = DOCS / "history"
    for f in md_files():
        if history in f.parents:
            continue
        in_fence = False
        for n, line in enumerate(f.read_text(encoding="utf-8").splitlines(), 1):
            if FENCE.match(line):
                in_fence = not in_fence
                continue
            if not in_fence:
                m = BAD_CHARS.search(line)
                if m:
                    problems.append(f"{rel(f)}:{n}: dash or curly quote U+{ord(m.group()):04X}")


def main():
    problems = []
    check_markdown_links(problems)
    check_html_refs(problems)
    check_diagram(problems)
    check_characters(problems)
    if problems:
        print(f"{len(problems)} problem(s):")
        for p in problems:
            print("  " + p)
        return 1
    print("docs ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
