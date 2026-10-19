import re
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools"))

import parity_inventory as pi  # noqa: E402


class ParityInventoryTest(unittest.TestCase):
    def test_parse_todos_extracts_id_title_and_files(self):
        text = (
            "- [x] **Todo 13a:** Short soak test in CI\n"
            "  - runs soak_mix.cpp headless\n"
            "- [ ] **Todo 8b:** FFmpeg decode backend\n"
        )
        todos = pi.parse_todos(text)
        self.assertEqual([t.id for t in todos], ["13a", "8b"])
        self.assertEqual(todos[0].title, "Short soak test in CI")
        self.assertIn("soak_mix.cpp", todos[0].files)
        self.assertEqual(todos[1].files, ())

    def test_status_comes_from_progress_items(self):
        progress = (
            '{"waves": [{"name": "2 Audio engine", "items": '
            '[["13a Soak (short)", "done"], ["26a Debug panel", "partial"]]}]}'
        )
        statuses = pi.parse_status(progress)
        self.assertEqual(statuses, {"13a": "done", "26a": "partial"})
        self.assertEqual(statuses.get("8b", "todo"), "todo")

    def test_port_sources_finds_cpp_names_in_rust_headers(self):
        text = "//! Port of tests/unit/LibraryDaoTest.cpp and `tools/ui_gate.cpp`.\n"
        self.assertEqual(pi.port_sources(text), {"LibraryDaoTest.cpp", "ui_gate.cpp"})

    def test_every_todo_has_a_mapping_entry_and_paths_exist(self):
        todo_ids = {t.id for t in pi.parse_todos(pi.tag_file("TODO.md"))}
        self.assertEqual(pi.check_mapping(todo_ids, pi.RUST_TESTS, ROOT), [])

    def test_check_mapping_reports_missing_and_unknown_entries(self):
        problems = pi.check_mapping(
            {"1", "2"},
            {"1": ("no/such/file.rs",), "3": ()},
            ROOT,
        )
        self.assertTrue(any("Todo 2 has no" in p for p in problems), problems)
        self.assertTrue(any("entry 3 is not" in p for p in problems), problems)
        self.assertTrue(any("no/such/file.rs does not exist" in p for p in problems), problems)

    def test_not_in_baseline_rows_say_so(self):
        todos = [pi.Todo("37", "Stem-aware mixing DSP", ()), pi.Todo("10", "Mixer", ())]
        rows = pi.build_rows(todos, {}, {"10": ()}, set(), lambda rel: "")
        self.assertEqual(rows[0][2], pi.NOT_IN_BASELINE_TEXT)
        self.assertEqual(rows[0][4], "todo")
        self.assertEqual(rows[1][3], "—")

    def test_every_todo_has_a_row(self):
        todo_ids = {t.id for t in pi.parse_todos(pi.tag_file("TODO.md"))}
        parity = (ROOT / "docs" / "parity.md").read_text(encoding="utf-8")
        row_ids = set(re.findall(r"^\| (\d+[a-z]?) \|", parity, re.M))
        self.assertEqual(todo_ids, row_ids)

    def test_every_row_has_a_status(self):
        parity = (ROOT / "docs" / "parity.md").read_text(encoding="utf-8")
        rows = re.findall(r"^\| (\d+[a-z]?) \|.* \| (done|partial|todo) \| .+ \|$", parity, re.M)
        row_ids = re.findall(r"^\| (\d+[a-z]?) \|", parity, re.M)
        self.assertEqual(sorted(i for i, _ in rows), sorted(row_ids))

    def test_cpp_status_parenthetical_is_dropped_from_title(self):
        self.assertEqual(
            pi.rust_title("Metadata ingest + watch folders (partial: RIFF INFO tags, duration)"),
            "Metadata ingest + watch folders",
        )
        self.assertEqual(pi.rust_title("Stems precompute (Demucs ONNX) (partial: x)"), "Stems precompute (Demucs ONNX)")
        self.assertEqual(pi.rust_title("Short soak test in CI (30-60s headless mix)"), "Short soak test in CI (30-60s headless mix)")

    def test_rust_note_replaces_the_cpp_parenthetical(self):
        todos = [pi.Todo("21", "Metadata ingest + watch folders (partial: TagLib tags open)", ())]
        rows = pi.build_rows(todos, {"21": "done"}, {}, set(), lambda rel: "")
        self.assertEqual(rows[0][1], "Metadata ingest + watch folders")
        self.assertEqual(rows[0][5], pi.RUST_NOTES["21"])
        self.assertNotIn("TagLib", "".join(rows[0]))

    def test_rust_notes_name_known_todos_and_stay_on_one_line(self):
        todo_ids = {t.id for t in pi.parse_todos(pi.tag_file("TODO.md"))}
        self.assertEqual(set(pi.RUST_NOTES) - todo_ids, set())
        for todo_id, note in pi.RUST_NOTES.items():
            self.assertNotIn("|", note, todo_id)
            self.assertNotIn("\n", note, todo_id)


if __name__ == "__main__":
    unittest.main()
