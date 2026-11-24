"""Builds the crate dependency diagram from `cargo metadata`, so it cannot drift from the code.

Writes docs/diagrams/crates.svg and docs/diagrams/crates.mmd (Mermaid, for GitHub) and fills the
`<!--CRATES_SVG-->` marker of docs/architecture.template.html into docs/architecture.html.

    python -I scripts/gen_arch.py
"""
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
RUST = ROOT / "scanner"
DOCS = ROOT / "docs"

ROLE = {
    "baocr": "애드온 루프, 스캔, doctor (바이너리)",
    "baocr-assist": "옵저버, 위젯, 어시스턴트, 설정",
    "baocr-overlay": "레이어드 창, 카드, 도킹, 트레이",
    "baocr-hcu": "데스크톱 제어, 스캔 루프, 안전장치",
    "baocr-extract": "프로필 읽기, 화면 종류, 레이드 트래커",
    "baocr-ocr": "ONNX Runtime의 PP-OCR, 닫힌 집합 CTC",
    "baocr-pixels": "픽셀 분류기, 프레임 키, 전투",
    "baocr-wikidb": "SchaleDB 수집기, DB, 조회",
    "baocr-pipeline": "비동기 추출 워커",
    "baocr-export": "JSON 저장소와 색인",
    "baocr-names": "학생 이름 사전",
    "baocr-equipment": "장비 아이콘 읽기",
    "baocr-core": "데이터 모델, Ocr 트레이트",
    "baocr-namedict": "도구: 이름 사전을 만든다",
    "baocr-progress": "진행 상태 읽기와 BAReceipt 내보내기",
}
GROUP = {  # colour by layer of responsibility
    "baocr": "#1f3a63",
    "baocr-assist": "#2f6f9f", "baocr-overlay": "#2f6f9f",
    "baocr-hcu": "#8a5a1c", "baocr-ocr": "#6b3f8f", "baocr-extract": "#6b3f8f", "baocr-pixels": "#6b3f8f",
    "baocr-names": "#6b3f8f", "baocr-equipment": "#6b3f8f", "baocr-pipeline": "#6b3f8f", "baocr-export": "#3f7f4f",
    "baocr-wikidb": "#3f7f4f", "baocr-core": "#555555", "baocr-namedict": "#777777", "baocr-progress": "#6b3f8f",
}


def metadata():
    out = subprocess.run(["cargo", "metadata", "--no-deps", "--format-version", "1"], cwd=RUST, capture_output=True, check=True).stdout
    return json.loads(out)


def loc(pkg):
    src = pathlib.Path(pkg["manifest_path"]).parent / "src"
    n = 0
    for f in src.rglob("*.rs"):
        n += len(f.read_bytes().splitlines())
    return n


def main():
    m = metadata()
    pk = {p["name"]: p for p in m["packages"]}
    edges = sorted({(n, d["name"]) for n, p in pk.items() for d in p["dependencies"] if d["name"] in pk and d["kind"] is None})
    users = {n: set() for n in pk}
    for a, b in edges:
        users[b].add(a)
    depth = {}

    def level(n):
        if n not in depth:
            depth[n] = 0 if not users[n] else 1 + max(level(u) for u in users[n])
        return depth[n]

    for n in pk:
        level(n)
    rows = {}
    for n in sorted(pk, key=lambda n: (depth[n], n)):
        rows.setdefault(depth[n], []).append(n)
    bw, bh, gx, gy = 196, 58, 22, 54
    width = max(len(r) for r in rows.values()) * (bw + gx) + gx
    height = (len(rows)) * (bh + gy) + gy
    pos = {}
    for d, names in rows.items():
        total = len(names) * (bw + gx) - gx
        x0 = (width - total) / 2
        for i, n in enumerate(names):
            pos[n] = (x0 + i * (bw + gx), gy / 2 + d * (bh + gy))
    svg = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {width} {height}" role="img" aria-label="crate dependency graph" font-family="Pretendard, Segoe UI, sans-serif">',
           '<defs><marker id="ah" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" fill="#8b97a8"/></marker></defs>']
    for a, b in edges:
        ax, ay = pos[a]
        bx, by = pos[b]
        x1, y1 = ax + bw / 2, ay + bh
        x2, y2 = bx + bw / 2, by
        mid = (y1 + y2) / 2
        svg.append(f'<path d="M{x1:.0f} {y1:.0f} C{x1:.0f} {mid:.0f} {x2:.0f} {mid:.0f} {x2:.0f} {y2 - 2:.0f}" fill="none" stroke="#8b97a8" stroke-width="1.2" marker-end="url(#ah)" opacity="0.8"/>')
    for n, (x, y) in pos.items():
        col = GROUP.get(n, "#555")
        svg.append(f'<g><rect x="{x:.0f}" y="{y:.0f}" width="{bw}" height="{bh}" rx="8" fill="{col}"/>')
        svg.append(f'<text x="{x + bw / 2:.0f}" y="{y + 22:.0f}" fill="#fff" font-size="14" font-weight="700" text-anchor="middle">{n.replace("baocr-", "") or n}</text>')
        svg.append(f'<text x="{x + bw / 2:.0f}" y="{y + 40:.0f}" fill="#dbe4f0" font-size="10.5" text-anchor="middle">{ROLE.get(n, "")}</text>')
        svg.append(f'<text x="{x + bw - 8:.0f}" y="{y + 52:.0f}" fill="#b7c4d6" font-size="9" text-anchor="end">{loc(pk[n])}줄</text></g>')
    svg.append("</svg>")
    svg_text = "\n".join(svg)
    (DOCS / "diagrams").mkdir(exist_ok=True)
    (DOCS / "diagrams" / "crates.svg").write_text(svg_text, encoding="utf-8")
    mmd = ["graph TD"] + [f"  {a.replace('-', '_')}[{a}] --> {b.replace('-', '_')}[{b}]" for a, b in edges]
    (DOCS / "diagrams" / "crates.mmd").write_text("\n".join(mmd) + "\n", encoding="utf-8")
    tpl = DOCS / "architecture.template.html"
    if tpl.exists():
        html = tpl.read_text(encoding="utf-8").replace("<!--CRATES_SVG-->", svg_text)
        (DOCS / "architecture.html").write_text(html, encoding="utf-8")
    print(f"{len(pk)} crates, {len(edges)} edges, depth {max(depth.values()) + 1}")


if __name__ == "__main__":
    main()
