#!/usr/bin/env python3
"""Copy the fixed Arena sources and emit their deterministic package manifest."""

import argparse
import json
from pathlib import Path

from arena_content import inspect_package, stage_package
from arena_macos import PROJECT, ROOT, write_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=ROOT / "apps/demo-game/content")
    parser.add_argument("--destination", type=Path,
                        default=PROJECT / "Assets/StreamingAssets/KituArena")
    parser.add_argument("--inspect", action="store_true", help="Validate the destination without writing it")
    parser.add_argument("--evidence", type=Path)
    args = parser.parse_args()
    result = inspect_package(args.destination) if args.inspect else stage_package(args.source, args.destination)
    if args.evidence:
        write_json(args.evidence, result)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
