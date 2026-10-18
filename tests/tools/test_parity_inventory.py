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

    def test_every_todo_has_a_row(self):
        todo_ids = {t.id for t in pi.parse_todos(pi.tag_file("TODO.md"))}
        parity = (ROOT / "docs" / "parity.md").read_text(encoding="utf-8")
        row_ids = set(re.findall(r"^\| (\d+[a-z]?) \|", parity, re.M))
        self.assertEqual(todo_ids, row_ids)


if __name__ == "__main__":
    unittest.main()
