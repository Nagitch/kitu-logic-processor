#!/usr/bin/env python3
"""Compare exact tick traces from the native determinism probe."""
import argparse
import json
import sys
from pathlib import Path


def load_trace(path, tick_count):
    rows = []
    with path.open(encoding="utf-8") as source:
        for tick, line in enumerate(source):
            try:
                row = json.loads(line)
            except ValueError as error:
                raise ValueError(f"{path}: invalid JSON at tick {tick}: {error}") from error
            actual = row.get("tick") if isinstance(row, dict) else None
            if type(actual) is not int or actual != tick:
                raise ValueError(f"{path}: expected tick {tick}, found {actual!r}")
            rows.append(row)
    if len(rows) != tick_count:
        raise ValueError(
            f"{path}: expected {tick_count} ticks, found {len(rows)} "
            f"(first missing or extra tick {min(len(rows), tick_count)})"
        )
    return rows


def first_difference(left, right, field="state"):
    if type(left) is not type(right):
        return field, left, right
    if isinstance(left, dict):
        for key in sorted(left.keys() | right.keys()):
            path = f"{field}.{key}"
            if key not in left or key not in right:
                return path, left.get(key, "<missing>"), right.get(key, "<missing>")
            difference = first_difference(left[key], right[key], path)
            if difference is not None:
                return difference
    elif isinstance(left, list):
        if len(left) != len(right):
            return f"{field}.length", len(left), len(right)
        for index, (a, b) in enumerate(zip(left, right)):
            entry = f"{field}[{index}]"
            if isinstance(a, dict) and isinstance(a.get("id"), str):
                entry += f"[id={a['id']}]"
            difference = first_difference(a, b, entry)
            if difference is not None:
                return difference
    elif left != right:
        return field, left, right
    return None


def compare(paths, tick_count):
    traces = [load_trace(path, tick_count) for path in paths]
    for tick, (reference, *others) in enumerate(zip(*traces)):
        for path, observed in zip(paths[1:], others):
            difference = first_difference(reference, observed)
            if difference is not None:
                field, left, right = difference
                raise ValueError(
                    f"tick {tick}, {field}: {paths[0]} = {left!r}, {path} = {right!r}"
                )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ticks", type=int, required=True)
    parser.add_argument("traces", type=Path, nargs="+")
    args = parser.parse_args()
    if args.ticks < 1 or len(args.traces) < 2:
        parser.error("provide a positive tick count and at least two traces")
    try:
        compare(args.traces, args.ticks)
    except (OSError, ValueError) as error:
        print(error, file=sys.stderr)
        return 1
    print(f"{len(args.traces)} traces matched exactly for {args.ticks} ticks")
    return 0


if __name__ == "__main__":
    sys.exit(main())
