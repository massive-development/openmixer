#!/usr/bin/env python3
"""Render docs/progress.json to docs/progress-heatmap.svg (one cell per task, one row per wave)."""
import json, pathlib
from xml.sax.saxutils import escape

ROOT = pathlib.Path(__file__).resolve().parent.parent
COLORS = {"done": "#2da44e", "partial": "#bf8700", "wip": "#fb8500", "todo": "#d0d7de"}
LABELS = {"done": "Done", "partial": "Partial", "wip": "In progress", "todo": "Not started"}
SCORE = {"done": 1.0, "partial": 0.5, "wip": 0.25, "todo": 0.0}
CELL_W, CELL_H, GAP, LABEL_W, PAD, PCT_W = 118, 44, 6, 120, 16, 56

data = json.loads((ROOT / "docs" / "progress.json").read_text())
waves = data["waves"]
cols = max(len(w["items"]) for w in waves)
width = PAD * 2 + LABEL_W + cols * (CELL_W + GAP) + PCT_W
top = PAD + 28
height = top + len(waves) * (CELL_H + GAP) + 44

out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" font-family="-apple-system,Segoe UI,Helvetica,Arial,sans-serif">',
       f'<rect width="100%" height="100%" fill="#ffffff"/>',
       f'<text x="{PAD}" y="{PAD + 12}" font-size="15" font-weight="600" fill="#1f2328">OpenMixer progress heatmap</text>']
total = done = 0.0
for r, w in enumerate(waves):
    y = top + r * (CELL_H + GAP)
    score = sum(SCORE[s] for _, s in w["items"])
    total += len(w["items"]); done += score
    out.append(f'<text x="{PAD}" y="{y + CELL_H / 2 + 4}" font-size="12" font-weight="600" fill="#1f2328">{escape(w["name"])}</text>')
    for c, (name, status) in enumerate(w["items"]):
        x = PAD + LABEL_W + c * (CELL_W + GAP)
        fg = "#ffffff" if status != "todo" else "#57606a"
        out.append(f'<g><title>{escape(name)}: {LABELS[status]}</title>'
                   f'<rect x="{x}" y="{y}" width="{CELL_W}" height="{CELL_H}" rx="5" fill="{COLORS[status]}"/>'
                   f'<text x="{x + CELL_W / 2}" y="{y + CELL_H / 2 + 4}" font-size="11" text-anchor="middle" fill="{fg}">{escape(name)}</text></g>')
    px = PAD + LABEL_W + cols * (CELL_W + GAP)
    out.append(f'<text x="{px}" y="{y + CELL_H / 2 + 4}" font-size="12" fill="#57606a">{round(100 * score / len(w["items"]))}%</text>')

ly = top + len(waves) * (CELL_H + GAP) + 12
lx = PAD + LABEL_W
for status in ("done", "partial", "wip", "todo"):
    out.append(f'<rect x="{lx}" y="{ly}" width="14" height="14" rx="3" fill="{COLORS[status]}"/>'
               f'<text x="{lx + 20}" y="{ly + 12}" font-size="12" fill="#1f2328">{LABELS[status]}</text>')
    lx += 120
out.append(f'<text x="{width - PAD}" y="{ly + 12}" font-size="12" text-anchor="end" fill="#1f2328">Overall: {round(100 * done / total)}% (weighted)</text>')
out.append('</svg>')
(ROOT / "docs" / "progress-heatmap.svg").write_text("\n".join(out) + "\n")
print(f"wrote docs/progress-heatmap.svg ({round(100 * done / total)}% overall)")
