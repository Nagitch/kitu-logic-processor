#!/usr/bin/env python3
"""Compare complete native C output against the same-build Runtime oracle trace."""
import argparse
import itertools
import json
from pathlib import Path


def compare(expected_path, actual_path):
    ticks = 0
    with expected_path.open() as expected, actual_path.open() as actual:
        for line, (left, right) in enumerate(itertools.zip_longest(expected, actual), 1):
            if left is None or right is None:
                raise ValueError(f"trace length differs at line {line}")
            left, right = json.loads(left), json.loads(right)
            if left != right:
                raise ValueError(f"complete output/state mismatch at line {line}, tick {left.get('tick')}")
            if left["tick"] != ticks:
                raise ValueError(f"non-contiguous tick at line {line}")
            ticks += 1
    if not ticks:
        raise ValueError("empty traces do not verify a scenario")
    return {"ticks": ticks, "stateAndOutput": "exact match", "expected": str(expected_path), "actual": str(actual_path)}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("expected", type=Path)
    parser.add_argument("actual", type=Path)
    args = parser.parse_args()
    print(json.dumps(compare(args.expected, args.actual), indent=2))
