#!/usr/bin/env python3
"""Render the GitHub issues as a heatmap SVG: one cell per issue, one block per milestone.

Cell colour is the issue's state:
  verified          closed as completed
  built-unverified  open, label `status:built-unverified` (built, not yet checked by ear/eye/hardware)
  partial           open, label `status:partial`
  not-started       open, label `status:not-started` or no status label

Usage
  python3 tools/issue_heatmap.py > issue-heatmap.svg          # reads the issues with `gh`
  python3 tools/issue_heatmap.py --from-json issues.json      # `gh issue list --state all --json
                                                              #   number,title,state,stateReason,labels,milestone`
Stdlib only. Issues closed as "not planned" are left out. Issues with no milestone are left out.
"""
import json
import subprocess
import sys
from xml.sax.saxutils import escape

MILESTONES = ["foundation", "alpha", "beta", "1.0"]
STATES = ["verified", "built-unverified", "partial", "not-started"]
COLORS = {"verified": "#2da44e", "built-unverified": "#0969da", "partial": "#bf8700", "not-started": "#d0d7de"}
NAMES = {"verified": "Verified (closed)", "built-unverified": "Built, unverified", "partial": "Partial",
         "not-started": "Not started"}
CELL_W, CELL_H, GAP, LABEL_W, PAD, COLS = 54, 30, 4, 110, 16, 24


def state_of(issue):
    if issue["state"] == "CLOSED":
        return None if issue.get("stateReason") == "NOT_PLANNED" else "verified"
    labels = {label["name"] for label in issue["labels"]}
    for name in ("built-unverified", "partial", "not-started"):
        if f"status:{name}" in labels:
            return name
    return "not-started"


def short(title):
    return title.split(" ", 1)[0]


def load():
    if "--from-json" in sys.argv:
        return json.load(open(sys.argv[sys.argv.index("--from-json") + 1]))
    out = subprocess.run(
        ["gh", "issue", "list", "--state", "all", "--limit", "1000", "--json",
         "number,title,state,stateReason,labels,milestone"],
        check=True, capture_output=True, text=True).stdout
    return json.loads(out)


def main():
    blocks = {m: [] for m in MILESTONES}
    for issue in load():
        milestone = (issue.get("milestone") or {}).get("title")
        state = state_of(issue)
        if milestone in blocks and state:
            blocks[milestone].append((issue["number"], issue["title"], state))
    for cells in blocks.values():
        cells.sort()

    rows = sum(max(1, -(-len(c) // COLS)) for c in blocks.values())
    width = PAD * 2 + LABEL_W + COLS * (CELL_W + GAP)
    top = PAD + 30
    height = top + rows * (CELL_H + GAP) + len(blocks) * 14 + 46
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" '
           f'viewBox="0 0 {width} {height}" font-family="-apple-system,Segoe UI,Helvetica,Arial,sans-serif">',
           '<rect width="100%" height="100%" fill="#ffffff"/>',
           f'<text x="{PAD}" y="{PAD + 12}" font-size="15" font-weight="600" fill="#1f2328">'
           'OpenMixer issue heatmap</text>']
    y = top
    totals = {s: 0 for s in STATES}
    for milestone in MILESTONES:
        cells = blocks[milestone]
        closed = sum(1 for _, _, s in cells if s == "verified")
        for _, _, s in cells:
            totals[s] += 1
        out.append(f'<text x="{PAD}" y="{y + CELL_H / 2 + 4}" font-size="12" font-weight="600" '
                   f'fill="#1f2328">{escape(milestone)}</text>')
        out.append(f'<text x="{PAD}" y="{y + CELL_H / 2 + 20}" font-size="11" fill="#57606a">'
                   f'{closed}/{len(cells)} verified</text>')
        for i, (number, title, state) in enumerate(cells):
            cx = PAD + LABEL_W + (i % COLS) * (CELL_W + GAP)
            cy = y + (i // COLS) * (CELL_H + GAP)
            fg = "#57606a" if state == "not-started" else "#ffffff"
            out.append(f'<g><title>#{number} {escape(title)}: {NAMES[state]}</title>'
                       f'<rect x="{cx}" y="{cy}" width="{CELL_W}" height="{CELL_H}" rx="4" fill="{COLORS[state]}"/>'
                       f'<text x="{cx + CELL_W / 2}" y="{cy + CELL_H / 2 + 4}" font-size="10" '
                       f'text-anchor="middle" fill="{fg}">{escape(short(title))}</text></g>')
        y += max(1, -(-len(cells) // COLS)) * (CELL_H + GAP) + 14
    lx = PAD + LABEL_W
    for state in STATES:
        out.append(f'<rect x="{lx}" y="{y}" width="14" height="14" rx="3" fill="{COLORS[state]}"/>'
                   f'<text x="{lx + 20}" y="{y + 12}" font-size="12" fill="#1f2328">{NAMES[state]} '
                   f'({totals[state]})</text>')
        lx += 190
    out.append('</svg>')
    sys.stdout.write("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
