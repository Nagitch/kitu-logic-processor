#!/usr/bin/env python3
"""Run the built graphical Player's deterministic native self-test and retain evidence."""

import argparse
import datetime
import json
import os
from pathlib import Path
import plistlib
import sys
import time

from arena_macos import ROOT, LIBRARY, artifact, require_macos, run, verify_plugin, write_json


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--player", required=True, type=Path)
    parser.add_argument("--trace", required=True, type=Path)
    parser.add_argument("--expected", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--timeout", type=float, default=600)
    parser.add_argument("player_args", nargs=argparse.REMAINDER,
                        help="Additional Player arguments after --")
    args = parser.parse_args()
    extras = args.player_args[1:] if args.player_args[:1] == ["--"] else args.player_args
    reserved = {"--arena-self-test", "--arena-expected", "--arena-evidence"}
    if any(value.split("=", 1)[0] in reserved for value in extras):
        parser.error("Additional Player arguments must not override the self-test trace, expected output or evidence directory")
    require_macos()
    player, trace, expected, evidence = [path.resolve() for path in
                                        [args.player, args.trace, args.expected, args.evidence]]
    with (player / "Contents/Info.plist").open("rb") as source:
        name = plistlib.load(source)["CFBundleExecutable"]
    if Path(name).name != name:
        raise RuntimeError("Invalid Player executable name")
    executable = player / "Contents/MacOS" / name
    plugins = list(player.rglob(LIBRARY))
    if len(plugins) != 1:
        raise RuntimeError("Player must contain exactly one native Arena library")
    plugin = verify_plugin(plugins[0])
    tick_count = input_count = 0
    with trace.open() as source:
        for line in source:
            if line.rstrip("\r\n") == "T":
                tick_count += 1
            elif line.startswith("I"):
                input_count += 1
            else:
                raise RuntimeError("Malformed input trace record")
    with expected.open() as source:
        if sum(1 for _ in source) != tick_count or not tick_count:
            raise RuntimeError("Expected output must cover every nonempty trace tick")
    evidence.mkdir(parents=True, exist_ok=True)
    result_path = evidence / "result.json"
    result_path.unlink(missing_ok=True)
    actual_path = evidence / "actual.ndjson"
    actual_path.unlink(missing_ok=True)
    command = [executable, "-screen-fullscreen", "0", "-screen-width", "1100",
               "-screen-height", "720", "-logFile", evidence / "player.log",
               *extras, "--arena-self-test", trace, "--arena-expected", expected,
               "--arena-evidence", evidence]
    env = os.environ.copy()
    env.pop("KITU_ARENA_WS_URL", None)
    started = time.time()
    run(command, log=evidence / "launch.log", env=env, timeout=args.timeout,
        cwd=player.parent)
    result = json.loads(result_path.read_text())
    if (result.get("matched") is not True or result.get("backend") != "native"
            or result.get("ticks") != tick_count or result.get("inputs") != input_count):
        raise RuntimeError(f"Player did not verify the complete native scenario: {result}")
    comparison = json.loads(run([sys.executable, ROOT / "tools/verify-arena-native.py",
                                 expected, actual_path]))
    screenshots = [artifact(path) for path in sorted(evidence.glob("*.png"))
                   if path.stat().st_mtime >= started]
    if not screenshots:
        raise RuntimeError("Graphical Player produced no current-run screenshot evidence")
    required = {"inventory.png"}
    if trace.stem == "stock-eleven-death-retry":
        required = {"chest.png", "combat.png", "boss.png", "death-11f.png", "retry.png"}
    elif trace.stem == "rhai-boss":
        required = {"boss-script.png"}
    captured = {Path(record["path"]).name for record in screenshots}
    if not required.issubset(captured):
        raise RuntimeError(f"Missing rendered checkpoints: {sorted(required - captured)}")
    report = {"verifiedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "elapsedSeconds": time.time() - started, "result": result,
              "executable": artifact(executable), "bundledPlugin": plugin,
              "trace": artifact(trace), "expected": artifact(expected),
              "actual": artifact(actual_path), "independentComparison": comparison,
              "screenshots": screenshots, "externalWebSocketOverrideRemoved": True,
              "command": [str(value) for value in command]}
    write_json(evidence / "player-verification.json", report)
    print(json.dumps({"ticks": tick_count, "inputs": input_count, "matched": True,
                      "evidence": str(evidence / "player-verification.json")}, indent=2))


if __name__ == "__main__":
    main()
