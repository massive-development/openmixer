import contextlib
import importlib
import io
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))

import check_docs_rust as cdr  # noqa: E402

DOC_FILES = [
    "README.md",
    "agents.md",
    "CONTRIBUTING.md",
    "TODO.md",
    "ROADMAP.md",
    "docs/PROGRESS.md",
    "docs/DESIGN_PATTERNS.md",
]
README_OK = "AGPL clean-room Spotify cargo\n"


def clean_docs():
    docs = {name: "Plain Rust project notes.\n" for name in DOC_FILES}
    docs["README.md"] = README_OK
    return docs


class CheckDocsRustTest(unittest.TestCase):
    def test_plain_violation_is_flagged_with_line_number(self):
        docs = clean_docs()
        docs["ROADMAP.md"] = "Phase 1\nBuilt with CMake and ctest.\n"
        violations = cdr.find_violations(docs)
        self.assertEqual(len(violations), 2)
        self.assertTrue(
            any(v.startswith("ROADMAP.md:2:") and "CMake" in v for v in violations),
            violations,
        )
        self.assertTrue(any("ctest" in v for v in violations), violations)

    def test_each_forbidden_term_is_flagged(self):
        terms = ["C++", "JUCE", "CMake", "cmake", "ctest", "g++", "clang-tidy",
                 ".cpp", ".hpp", "GoogleTest", "webkit"]
        for term in terms:
            with self.subTest(term=term):
                docs = clean_docs()
                docs["TODO.md"] = f"see {term} here\n"
                violations = cdr.find_violations(docs)
                self.assertTrue(
                    any(v.startswith("TODO.md:1:") for v in violations),
                    violations,
                )

    def test_forbidden_terms_are_case_sensitive(self):
        docs = clean_docs()
        docs["TODO.md"] = "Juce and Cmake and Webkit are fine here\n"
        self.assertEqual(cdr.find_violations(docs), [])

    def test_same_text_inside_history_fence_passes(self):
        docs = clean_docs()
        docs["docs/PROGRESS.md"] = (
            "Intro\n"
            "```history\n"
            "Old build used CMake and g++.\n"
            "See src/main.cpp\n"
            "```\n"
            "After\n"
        )
        self.assertEqual(cdr.find_violations(docs), [])

    def test_violation_after_history_fence_is_still_flagged(self):
        docs = clean_docs()
        docs["docs/PROGRESS.md"] = (
            "```history\nCMake\n```\n"
            "Now we use CMake again\n"
        )
        violations = cdr.find_violations(docs)
        self.assertEqual(len(violations), 1, violations)
        self.assertTrue(violations[0].startswith("docs/PROGRESS.md:4:"))

    def test_non_history_fence_is_not_exempt(self):
        docs = clean_docs()
        docs["agents.md"] = "```bash\ncmake --build build\n```\n"
        violations = cdr.find_violations(docs)
        self.assertTrue(any(v.startswith("agents.md:2:") for v in violations),
                        violations)

    def test_marker_line_is_exempt(self):
        docs = clean_docs()
        docs["CONTRIBUTING.md"] = (
            "Old text mentions CMake and JUCE. <!-- cpp-history -->\n"
            "New text mentions CMake.\n"
        )
        violations = cdr.find_violations(docs)
        self.assertEqual(len(violations), 1, violations)
        self.assertTrue(violations[0].startswith("CONTRIBUTING.md:2:"))

    def test_missing_readme_required_terms_are_flagged(self):
        docs = clean_docs()
        docs["README.md"] = "AGPL and Spotify only\n"
        violations = cdr.find_violations(docs)
        self.assertEqual(len(violations), 2, violations)
        for term in ["clean-room", "cargo"]:
            self.assertTrue(any(term in v for v in violations), (term, violations))

    def test_clean_room_match_is_case_insensitive(self):
        docs = clean_docs()
        docs["README.md"] = "AGPL Clean-Room Spotify cargo\n"
        self.assertEqual(cdr.find_violations(docs), [])

    def test_required_terms_only_apply_to_readme(self):
        docs = clean_docs()
        docs["TODO.md"] = "nothing special\n"
        self.assertEqual(cdr.find_violations(docs), [])

    def test_missing_file_is_flagged(self):
        docs = clean_docs()
        del docs["ROADMAP.md"]
        violations = cdr.find_violations(docs)
        self.assertEqual(len(violations), 1, violations)
        self.assertIn("ROADMAP.md", violations[0])
        self.assertIn("missing", violations[0])

    def test_missing_readme_reports_only_file_missing(self):
        docs = clean_docs()
        del docs["README.md"]
        violations = cdr.find_violations(docs)
        self.assertEqual(len(violations), 1, violations)
        self.assertIn("README.md", violations[0])

    def test_clean_docs_pass(self):
        self.assertEqual(cdr.find_violations(clean_docs()), [])

    def test_main_prints_docs_ok_and_returns_zero_when_clean(self):
        self._run_main_in_tree(clean_docs(), expect_code=0, expect_out="docs ok")

    def test_main_prints_violations_and_returns_one(self):
        docs = clean_docs()
        docs["TODO.md"] = "uses CMake\n"
        out = self._run_main_in_tree(docs, expect_code=1)
        self.assertIn("TODO.md:1:", out)
        self.assertNotIn("docs ok", out)

    def _run_main_in_tree(self, docs, expect_code, expect_out=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, text in docs.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text, encoding="utf-8")
            original = cdr.ROOT
            cdr.ROOT = root
            buf = io.StringIO()
            try:
                with contextlib.redirect_stdout(buf):
                    code = cdr.main()
            finally:
                cdr.ROOT = original
        self.assertEqual(code, expect_code)
        out = buf.getvalue()
        if expect_out is not None:
            self.assertIn(expect_out, out)
        return out


if __name__ == "__main__":
    unittest.main()
