#!/usr/bin/env python3
"""Rebuild graphify-out/ with Rust `use` edges resolved to file nodes.

graphify's Rust extractor records `use` targets as bare path segments (`super`, `om_core`,
`atomic`), which match no node and are dropped when the graph is built (~1.5k edges here). This
script re-resolves `crate::`, `super::`, `self::` and workspace-crate imports to module files, then
rebuilds graph.json, GRAPH_REPORT.md and graph.html. Imports of std/external crates stay dropped.

Run from anywhere with graphify's interpreter, after a `/graphify` run has cached the docs/images:
  $(cat graphify-out/.graphify_python) tools/graphify_rebuild.py

Community names come from graphify-out/community_labels.json ([{name, members}]); each new
community takes the name of the saved one it overlaps most (Jaccard >= 0.3), else "Community N".
Doc/image nodes come from graphify's semantic cache; uncached docs are skipped (re-run /graphify).
Nested .gitignore/.graphifyignore files are ignored on purpose: graphify applies a nested `*`
(e.g. .opencode/memory/.gitignore) to the whole tree and detects zero files. Community labels
are matched by member overlap because ids change when edges change.
"""
import json
import re
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SKIP_DIRS = {"target", ".claude", "graphify-out", "node_modules"}


def node_id(rel):
    return re.sub(r"[^a-z0-9]+", "_", str(Path(rel).with_suffix("")).lower()).strip("_")


def crate_map():
    crates = {}
    for toml in ROOT.glob("**/Cargo.toml"):
        if SKIP_DIRS & set(toml.relative_to(ROOT).parts):
            continue
        m = re.search(r'^\s*name\s*=\s*"([^"]+)"', toml.read_text(), re.M)
        if m and (toml.parent / "src").is_dir():
            crates[m.group(1).replace("-", "_")] = toml.parent / "src"
    return crates


def crate_src(path):
    for p in path.parents:
        if p == ROOT:
            break
        if (p / "Cargo.toml").exists() and (p / "src").is_dir():
            return p / "src"
    return None


def module_path(path, src):
    parts = list(path.relative_to(src).with_suffix("").parts)
    if parts and parts[-1] in ("mod", "lib", "main"):
        parts.pop()
    return parts


def resolve_mod(src, segs):
    """Deepest module file reached by walking segs from the crate root."""
    cur, found = src, None
    for s in segs:
        if (cur / f"{s}.rs").is_file():
            found, cur = cur / f"{s}.rs", cur / s
        elif (cur / s / "mod.rs").is_file():
            found, cur = cur / s / "mod.rs", cur / s
        else:
            break
    if found:
        return found
    return next((src / n for n in ("lib.rs", "main.rs") if (src / n).is_file()), None)


class Resolver:
    def __init__(self):
        self.crates = crate_map()
        self.lines = {}

    def use_path(self, path, line):
        ls = self.lines.setdefault(path, path.read_text(errors="ignore").splitlines())
        buf = ""
        for i in range(line - 1, min(len(ls), line + 60)):
            code = ls[i].split("//")[0]
            buf += " " + code
            if ";" in code:
                break
        m = re.search(r"\buse\s+(?:::)?([\w:]+)", buf)
        return m.group(1).split("::") if m else None

    def resolve(self, edge):
        if not edge.get("source_file", "").endswith(".rs"):
            return None
        f = ROOT / edge["source_file"]
        src = crate_src(f)
        try:
            line = int(edge["source_location"].lstrip("L"))
            segs = self.use_path(f, line)
        except (ValueError, AttributeError, OSError):
            return None
        if src is None or not segs:
            return None
        head, rest = segs[0], segs[1:]
        in_src = str(f).startswith(str(src))
        if head in ("crate", "self", "super"):
            if not in_src:
                return None
            mp = module_path(f, src)
            base, tsrc = ([], src) if head == "crate" else (mp, src)
            if head == "super":
                base = mp[:-1]
                while rest and rest[0] == "super":
                    base, rest = base[:-1], rest[1:]
        elif head in self.crates:
            base, tsrc = [], self.crates[head]
        else:
            return None
        t = resolve_mod(tsrc, base + rest)
        return node_id(t.relative_to(ROOT)) if t else None


def load_labels(com):
    """Name each community after the saved label whose members overlap it most."""
    labels = {c: f"Community {c}" for c in com}
    path = ROOT / "graphify-out" / "community_labels.json"
    if not path.is_file():
        return labels
    saved = [(e["name"], set(e["members"])) for e in json.loads(path.read_text())]
    taken = set()
    pairs = []
    for c, members in com.items():
        ms = set(members)
        for i, (_, old) in enumerate(saved):
            j = len(ms & old) / len(ms | old)
            if j >= 0.3:
                pairs.append((j, c, i))
    for _, c, i in sorted(pairs, reverse=True):
        if labels[c].startswith("Community ") and i not in taken:
            labels[c] = saved[i][0]
            taken.add(i)
    return labels


def main():
    from graphify import detect as D
    from graphify.analyze import god_nodes, suggest_questions, surprising_connections
    from graphify.build import build_from_json
    from graphify.cache import check_semantic_cache
    from graphify.cluster import cluster, score_all
    from graphify.export import to_json
    from graphify.extract import extract
    from graphify.report import generate

    own = D._load_dir_own_ignore
    D._load_dir_own_ignore = lambda d: own(d) if d.resolve() == ROOT else []
    det = D.detect(ROOT)
    ast = extract([Path(f) for f in det["files"]["code"]], cache_root=ROOT)

    ids = {n["id"] for n in ast["nodes"]}
    res, seen, edges = Resolver(), set(), []
    fixed = dropped = 0
    for e in ast["edges"]:
        if e["target"] in ids:
            edges.append(e)
            continue
        t = res.resolve(e)
        if t in ids:
            key = (e["source"], t, e["relation"], e.get("source_location"))
            if key not in seen:
                seen.add(key)
                edges.append(dict(e, target=t))
                fixed += 1
        else:
            dropped += 1
    print(f"imports: {fixed} resolved, {dropped} external/unresolvable dropped")

    docs = [f for c in ("document", "paper", "image") for f in det["files"].get(c, [])]
    sn, se, sh, uncached = check_semantic_cache(docs, root=str(ROOT))
    if uncached:
        print(f"warning: {len(uncached)} docs/images not cached; run /graphify to extract them")
    nodes = list(ast["nodes"])
    have = set(ids)
    nodes += [n for n in sn if n["id"] not in have and not have.add(n["id"])]
    ex = {"nodes": nodes, "edges": edges + se, "hyperedges": sh, "input_tokens": 0, "output_tokens": 0}

    out = ROOT / "graphify-out"
    G = build_from_json(ex, root=str(ROOT), directed=False)
    if G.number_of_nodes() == 0:
        raise SystemExit("empty graph")
    com = cluster(G)
    labels = load_labels(com)
    if not to_json(G, com, str(out / "graph.json")):
        raise SystemExit("refused to shrink graph.json")
    report = generate(G, com, score_all(G, com), labels, god_nodes(G), surprising_connections(G, com),
                      det, {"input": 0, "output": 0}, str(ROOT),
                      suggested_questions=suggest_questions(G, com, labels))
    (out / "GRAPH_REPORT.md").write_text(report, encoding="utf-8")
    print(f"graph: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges, {len(com)} communities")
    subprocess.run(["graphify", "export", "html"], cwd=ROOT, check=False)


if __name__ == "__main__":
    main()
