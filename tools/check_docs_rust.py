"""Verify the docs describe the Rust project, not the old C++/JUCE/CMake build.

Run from anywhere: python3 -I tools/check_docs_rust.py
Exits 1 and prints `file:line: message` per violation, else prints `docs ok`.
"""
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

DOC_FILES = [
    "README.md",
    "agents.md",
    "CONTRIBUTING.md",
    "TODO.md",
    "ROADMAP.md",
    "docs/PROGRESS.md",
    "docs/DESIGN_PATTERNS.md",
    "NOTICE",
    "fixtures/README.md",
]
FORBIDDEN = ["C++", "JUCE", "CMake", "cmake", "ctest", "g++", "clang-tidy",
             ".cpp", ".hpp", "GoogleTest", "webkit"]
HISTORY_FENCE = "```history"
HISTORY_MARKER = "<!-- cpp-history -->"
README = "README.md"
README_REQUIRED = ["AGPL", "clean-room", "Spotify", "cargo"]


def find_violations(path_to_text: dict[str, str]) -> list[str]:
    """Return `file:line: message` strings for every rule violation.

    `path_to_text` maps a repo-relative doc path to its full text. A doc in
    DOC_FILES that is absent from the mapping is reported as missing.
    """
    violations: list[str] = []
    for name in DOC_FILES:
        if name not in path_to_text:
            violations.append(f"{name}:0: required file missing")
    for name, text in path_to_text.items():
        if name not in DOC_FILES:
            continue
        violations.extend(_check_forbidden(name, text))
    if README in path_to_text:
        violations.extend(_check_readme_terms(path_to_text[README]))
    return violations


def _check_forbidden(name: str, text: str) -> list[str]:
    found: list[str] = []
    fence = None  # None, "history", or "other"
    for lineno, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        in_history = fence == "history"
        if fence is None:
            if stripped.startswith("```"):
                fence = "history" if stripped.startswith(HISTORY_FENCE) else "other"
        elif stripped == "```":
            fence = None
        if in_history or HISTORY_MARKER in line:
            continue
        for term in FORBIDDEN:
            if term in line:
                found.append(
                    f"{name}:{lineno}: forbidden term {term!r} "
                    "(C++/JUCE/CMake wording in the Rust project docs)"
                )
    return found


def _check_readme_terms(text: str) -> list[str]:
    found: list[str] = []
    lowered = text.lower()
    for term in README_REQUIRED:
        present = term.lower() in lowered if term == "clean-room" else term in text
        if not present:
            found.append(f"{README}:1: missing required term {term!r}")
    return found


def main() -> int:
    texts: dict[str, str] = {}
    for name in DOC_FILES:
        path = ROOT / name
        if path.is_file():
            texts[name] = path.read_text(encoding="utf-8")
    violations = find_violations(texts)
    for violation in violations:
        print(violation)
    if violations:
        return 1
    print("docs ok")
    return 0


if __name__ == "__main__":
    sys.exit(main())
