"""Generate docs/parity.md: one row per C++ Todo at the cpp-final tag.

Run from the repo root: python3 -I tools/parity_inventory.py
"""
import dataclasses
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TAG = "cpp-final"
TODO_RE = re.compile(r"\*\*Todo (\d+[a-z]?)")
FILE_RE = re.compile(r"[\w-]+\.\w+")
PARAGRAPH = (
    "This table is generated from the `cpp-final` git tag (the frozen C++ baseline) "
    "by `tools/parity_inventory.py`; do not edit it by hand. To regenerate, run "
    "`python3 -I tools/parity_inventory.py` from the repo root. Each row is one "
    "`**Todo N:**` entry in that tag's `TODO.md`. The C++ test column lists test and "
    "tool files under `tests/` and `tools/` that the Todo names explicitly. The Rust "
    "columns are placeholders until the Rust side is filled in."
)


@dataclass(frozen=True)
class Todo:
    id: str
    title: str
    files: tuple[str, ...]


def parse_todos(text: str) -> list[Todo]:
    lines = text.splitlines()
    todos = []
    for i, line in enumerate(lines):
        match = TODO_RE.search(line)
        if not match:
            continue
        block = [line]
        for follow in lines[i + 1:]:
            if not follow.strip() or follow[0] not in " \t":
                break
            block.append(follow)
        title = line.split(":", 1)[1].lstrip("* ").strip()
        files = tuple(dict.fromkeys(FILE_RE.findall("\n".join(block))))
        todos.append(Todo(match.group(1), title, files))
    return todos


def tag_file(path: str) -> str:
    return subprocess.run(
        ["git", "show", f"{TAG}:{path}"],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout


def tag_test_basenames() -> set[str]:
    out = subprocess.run(
        ["git", "ls-tree", "-r", "--name-only", TAG],
        cwd=ROOT, check=True, capture_output=True, text=True,
    ).stdout
    return {Path(p).name for p in out.splitlines() if p.startswith(("tests/", "tools/"))}


def render(todos: list[Todo]) -> str:
    lines = [
        "# C++ to Rust parity inventory",
        "",
        PARAGRAPH,
        "",
        "| Todo | Title | C++ test(s) | Rust test | Status |",
        "| --- | --- | --- | --- | --- |",
    ]
    for todo in todos:
        cpp = ", ".join(f"`{name}`" for name in todo.files) or "—"
        title = todo.title.replace("|", "\\|")
        lines.append(f"| {todo.id} | {title} | {cpp} | — | todo |")
    return "\n".join(lines) + "\n"


def main() -> None:
    known = tag_test_basenames()
    todos = [
        dataclasses.replace(t, files=tuple(f for f in t.files if f in known))
        for t in parse_todos(tag_file("TODO.md"))
    ]
    (ROOT / "docs" / "parity.md").write_text(render(todos), encoding="utf-8")


if __name__ == "__main__":
    main()
