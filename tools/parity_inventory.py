"""Generate docs/parity.md: one row per Todo, mapped to the C++ baseline and the Rust port.

Run from the repo root: python3 -I tools/parity_inventory.py

The C++ column comes from the `cpp-final` tag (the frozen baseline): the test and tool
files each Todo names, plus the `tests/unit` and `tools` sources that the Rust test files
say they port. The Rust column is the explicit mapping in RUST_TESTS below. The Status
column is read from docs/progress.json in the working tree.
"""
import json
import re
import subprocess
import sys
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TAG = "cpp-final"
TODO_RE = re.compile(r"\*\*Todo (\d+[a-z]?)")
FILE_RE = re.compile(r"[\w-]+\.\w+")
PORT_RE = re.compile(r"((?:tests/unit|tools)/[\w./-]+?\.cpp)")
NOT_IN_BASELINE_TEXT = "not in C++ baseline"
PARAGRAPH = (
    "This table is generated from the `cpp-final` git tag (the frozen C++ baseline) "
    "by `tools/parity_inventory.py`; do not edit it by hand. To regenerate, run "
    "`python3 -I tools/parity_inventory.py` from the repo root. Each row is one "
    "`**Todo N:**` entry in that tag's `TODO.md`. The C++ column lists the test and tool "
    "files that the Todo names, plus the `tests/unit` and `tools` sources the Rust tests "
    "say they port. The Rust column lists the crate test files (and inline test modules) "
    "that replace them. Status comes from `docs/progress.json`: `done`, `partial` or "
    "`todo`. Todos that the C++ baseline never implemented (Waves 7-9, and Todos 30, "
    "34-36) say so in the C++ column."
)

# Todo ids that the C++ baseline never implemented; their C++ column says so.
NOT_IN_BASELINE = frozenset(
    {"30", "34", "35", "36"} | {str(n) for n in range(37, 52)}
)

# Todo id -> Rust crate test files (and inline test modules) that replace the C++ tests.
# Every Todo in TODO.md must have an entry (an empty tuple means no Rust test yet).
RUST_TESTS: dict[str, tuple[str, ...]] = {
    "7": ("crates/om-audio/tests/audio_device.rs", "crates/om-audio/tests/cpal_device.rs"),
    "8": (
        "crates/om-engine/src/deck/tests.rs",
        "crates/om-engine/src/dj_engine/tests.rs",
        "crates/om-audio/tests/dj_engine.rs",
    ),
    "10": ("crates/om-engine/tests/mixer.rs",),
    "10a": (
        "crates/om-plugins/tests/plugin_registry.rs",
        "examples/plugins/gain-plugin/tests/gain_plugin.rs",
        "crates/openmixer/src/plugins.rs",
    ),
    "32": (
        "crates/om-midi/tests/midi_device.rs",
        "crates/om-midi/tests/midi_mapper.rs",
        "crates/om-midi/tests/midi_learn.rs",
        "crates/om-midi/tests/alsa_backend.rs",
    ),
    "33": ("crates/om-midi/tests/midi_mapper.rs", "fixtures/controllers"),
    "26a": ("crates/om-ui-gpui/src/debug_panel.rs",),
    "13a": ("crates/om-audio/tests/soak.rs",),
    "9": ("crates/om-engine/src/deck/keylock_tests.rs", "crates/om-stretch/src/rubberband.rs"),
    "12": (
        "crates/om-engine/src/sync.rs",
        "crates/om-engine/src/quantizer.rs",
        "crates/om-engine/src/slip.rs",
        "crates/om-engine/src/deck/loop_tests.rs",
        "crates/om-audio/tests/engine_wiring.rs",
    ),
    "11": (
        "crates/om-engine/tests/fx.rs",
        "crates/om-engine/src/fx/rack.rs",
        "crates/om-engine/src/fx_routing.rs",
    ),
    "8b": (),
    "13": ("crates/om-audio/tests/soak.rs",),
    "14": (
        "crates/om-analysis/src/service/tests.rs",
        "crates/om-analysis/src/sqlite_cache/tests.rs",
        "crates/om-analysis/src/registry/tests.rs",
        "crates/om-analysis/src/clip_source/tests.rs",
    ),
    "15": ("crates/om-analysis/src/bpm/tests.rs",),
    "16": ("crates/om-analysis/src/key/tests.rs",),
    "17": ("crates/om-analysis/src/waveform/tests.rs",),
    "18": ("crates/om-analysis/src/structure/tests.rs",),
    "19": ("crates/om-stems/tests/stems.rs",),
    "20": ("crates/om-library/tests/library_dao.rs", "crates/om-library/tests/library_database.rs"),
    "21": (
        "crates/om-library/tests/library_ingest.rs",
        "crates/om-library/tests/metadata_reader.rs",
        "crates/om-library/tests/library_watch_folders.rs",
    ),
    "22": ("crates/om-library/tests/library_import.rs",),
    "23": (
        "crates/om-library/tests/library_search.rs",
        "crates/om-library/tests/library_search_properties.rs",
    ),
    "24": ("crates/om-library/tests/library_prep_queue.rs",),
    "25a": ("crates/om-ui/tests/theme_json.rs", "crates/om-ui/tests/theme_registry.rs"),
    "25": ("crates/om-ui/src/theme/tokens.rs",),
    "26": ("crates/om-ui/tests/deck_widgets.rs", "crates/om-ui/tests/main_layout.rs"),
    "27": ("crates/om-ui/tests/mixer_widgets.rs",),
    "28": (
        "crates/om-ui/tests/library_browser.rs",
        "crates/om-ui/tests/track_loader.rs",
        "crates/om-ui-gpui/tests/library_panel.rs",
    ),
    "29": (
        "crates/om-ui/tests/library_settings.rs",
        "crates/om-ui/src/settings.rs",
        "crates/om-ui/src/preferences.rs",
        "crates/om-ui/src/preferences_applier.rs",
        "crates/openmixer/tests/audio_settings.rs",
        "crates/om-ui-gpui/src/settings_view.rs",
    ),
    "30": (),
    "31": ("crates/openmixer/tests/ui_gate.rs", "crates/openmixer/tests/smoke.rs"),
    "34": (),
    "35": (),
    "36": (),
    "37": (),
    "38": (),
    "39": (),
    "40": (),
    "41": (),
    "42": (),
    "43": (),
    "44": (),
    "47": (),
    "48": (),
    "49": (),
    "50": (),
    "51": (),
}


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


def parse_status(progress_text: str) -> dict[str, str]:
    """Map Todo id to status from docs/progress.json items named like `"7 Device layer"`."""
    statuses: dict[str, str] = {}
    for wave in json.loads(progress_text)["waves"]:
        for name, status in wave["items"]:
            statuses[name.split()[0]] = status
    return statuses


def port_sources(rust_text: str) -> set[str]:
    """C++ test and tool basenames that a Rust test file says it ports."""
    return {Path(p).name for p in PORT_RE.findall(rust_text)}


def check_mapping(todo_ids: set[str], mapping: dict[str, tuple[str, ...]], root: Path) -> list[str]:
    problems = []
    for todo_id in sorted(todo_ids - set(mapping), key=str):
        problems.append(f"Todo {todo_id} has no RUST_TESTS entry")
    for todo_id in sorted(set(mapping) - todo_ids, key=str):
        problems.append(f"RUST_TESTS entry {todo_id} is not a Todo in TODO.md")
    for todo_id, paths in mapping.items():
        for rel in paths:
            if not (root / rel).exists():
                problems.append(f"Todo {todo_id}: {rel} does not exist")
    return problems


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


def build_rows(
    todos: list[Todo],
    statuses: dict[str, str],
    mapping: dict[str, tuple[str, ...]],
    known: set[str],
    read_rust,
) -> list[tuple[str, str, str, str, str]]:
    rows = []
    for todo in todos:
        rust_paths = mapping.get(todo.id, ())
        if todo.id in NOT_IN_BASELINE:
            cpp_names = [NOT_IN_BASELINE_TEXT]
        else:
            ported: set[str] = set()
            for rel in rust_paths:
                if (ROOT / rel).is_file():
                    ported |= port_sources(read_rust(rel))
            names = {f for f in todo.files if f in known} | (ported & known)
            cpp_names = [f"`{name}`" for name in sorted(names)] or ["—"]
        rust_cells = [f"`{rel}`" for rel in rust_paths] or ["—"]
        title = todo.title.replace("|", "\\|")
        rows.append((
            todo.id,
            title,
            ", ".join(cpp_names),
            ", ".join(rust_cells),
            statuses.get(todo.id, "todo"),
        ))
    return rows


def render(rows: list[tuple[str, str, str, str, str]]) -> str:
    lines = [
        "# C++ to Rust parity inventory",
        "",
        PARAGRAPH,
        "",
        "| Todo | Title | C++ test(s) | Rust crate(s) / test file(s) | Status |",
        "| --- | --- | --- | --- | --- |",
    ]
    for todo_id, title, cpp, rust, status in rows:
        lines.append(f"| {todo_id} | {title} | {cpp} | {rust} | {status} |")
    return "\n".join(lines) + "\n"


def main() -> int:
    todos = parse_todos(tag_file("TODO.md"))
    problems = check_mapping({t.id for t in todos}, RUST_TESTS, ROOT)
    if problems:
        for problem in problems:
            print(problem, file=sys.stderr)
        return 1
    statuses = parse_status((ROOT / "docs" / "progress.json").read_text(encoding="utf-8"))
    rows = build_rows(
        todos,
        statuses,
        RUST_TESTS,
        tag_test_basenames(),
        lambda rel: (ROOT / rel).read_text(encoding="utf-8"),
    )
    (ROOT / "docs" / "parity.md").write_text(render(rows), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
