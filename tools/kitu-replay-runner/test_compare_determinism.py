"""Regression tests for exact native replay comparisons."""
import json
import tempfile
import unittest
from pathlib import Path

from compare_determinism import compare


class TraceComparisonTests(unittest.TestCase):
    def setUp(self):
        work = tempfile.TemporaryDirectory()
        self.addCleanup(work.cleanup)
        self.left = Path(work.name, "left.jsonl")
        self.right = Path(work.name, "right.jsonl")
        self.rows = [
            {"tick": 0, "position_um": [0, 0, 0],
             "bodies": [{"id": "box", "translation": [0, 0, 0]}]},
            {"tick": 1, "position_um": [1, 0, 0], "bodies": []},
        ]
        self.write(self.left, self.rows)

    def write(self, path, rows):
        path.write_text("".join(json.dumps(row) + "\n" for row in rows), encoding="utf-8")

    def test_matching_traces(self):
        self.write(self.right, self.rows)
        compare([self.left, self.right], 2)

    def test_rejects_single_micrometre(self):
        self.rows[1]["position_um"][0] += 1
        self.write(self.right, self.rows)
        with self.assertRaisesRegex(ValueError, r"tick 1, state\.position_um\[0\]"):
            compare([self.left, self.right], 2)

    def test_rejects_single_body_bit_and_names_body(self):
        self.rows[0]["bodies"][0]["translation"][0] ^= 1
        self.write(self.right, self.rows)
        with self.assertRaisesRegex(ValueError, r"tick 0, state\.bodies\[0\]\[id=box\]\.translation\[0\]"):
            compare([self.left, self.right], 2)

    def test_rejects_missing_and_out_of_order_ticks(self):
        for rows, message in [
            (self.rows[:1], "first missing or extra tick 1"),
            (self.rows[::-1], "expected tick 0, found 1"),
            ([self.rows[0], self.rows[0]], "expected tick 1, found 0"),
        ]:
            with self.subTest(message=message):
                self.write(self.right, rows)
                with self.assertRaisesRegex(ValueError, message):
                    compare([self.left, self.right], 2)

    def test_rejects_equal_value_with_different_json_type(self):
        self.rows[1]["position_um"][0] = True
        self.write(self.right, self.rows)
        with self.assertRaisesRegex(ValueError, r"tick 1, state\.position_um\[0\]"):
            compare([self.left, self.right], 2)


if __name__ == "__main__":
    unittest.main()
