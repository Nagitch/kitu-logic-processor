#!/usr/bin/env python3
"""Verify Arena oracle provenance and frozen fixture integrity using only stdlib."""

import hashlib
import json
import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "kitu-integration-runner/scenarios/arena/reference"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def finite_json(value):
    if isinstance(value, dict):
        for child in value.values():
            finite_json(child)
    elif isinstance(value, list):
        for child in value:
            finite_json(child)
    elif isinstance(value, float):
        require(math.isfinite(value), "non-finite number in reference JSON")


def read_json(path):
    value = json.loads(path.read_text())
    finite_json(value)
    return value


def main():
    baseline = read_json(FIXTURES / "baseline.json")
    for relative, expected in baseline["sourceSha256"].items():
        source = (ROOT / relative).read_text()
        # The only allowed changes to the two oracle rule files are serializable
        # attributes and partial declarations for detached diagnostic projections.
        source = source.replace("    [Serializable]\n", "").replace("sealed partial class", "sealed class")
        actual = hashlib.sha256(source.encode()).hexdigest()
        require(actual == expected, f"original rule source changed: {relative}")
    for relative, expected in baseline["fixtureSha256"].items():
        actual = hashlib.sha256((FIXTURES / relative).read_bytes()).hexdigest()
        require(actual == expected, f"frozen fixture changed: {relative}")

    for name in ("preparation", "stock-eleven-death-retry"):
        directory = FIXTURES / name
        scenario = read_json(directory / "scenario.json")
        require(scenario["schemaVersion"] == 1 and scenario["tickRate"] == 60, "unsupported reference schema/tick rate")
        require(scenario["baselineRevision"] == baseline["revision"], "baseline revision mismatch")
        require(scenario["scenarioId"] == name, "scenario identity mismatch")
        steps = scenario["steps"]
        require([step["tick"] for step in steps] == list(range(len(steps))), "non-contiguous input ticks")
        expected = [json.loads(line) for line in (directory / "expected.ndjson").read_text().splitlines()]
        outcomes = [json.loads(line) for line in (directory / "outcomes.ndjson").read_text().splitlines()]
        finite_json(expected)
        ticks = [state["tick"] for state in expected]
        require(ticks == sorted(set(ticks)), "duplicate/out-of-order checkpoints")
        require(ticks[0] == 0 and ticks[-1] == len(steps) - 1, "missing endpoint checkpoint")
        require(all(b - a <= 60 for a, b in zip(ticks, ticks[1:])), "checkpoint gap exceeds 60 ticks")
        commands = [(step["tick"], command["id"]) for step in steps for command in step["commands"]]
        require(commands == [(outcome["tick"], outcome["id"]) for outcome in outcomes], "outcome order mismatch")
        if name == "stock-eleven-death-retry":
            require(all(outcome["accepted"] for outcome in outcomes), "stock command rejected")
            deaths = [state for state in expected if state["result"]["present"]]
            require(bool(deaths), "missing natural death checkpoint")
            require(deaths[0]["result"]["floor"] == 11, "stock run must reach 11F")
            require(deaths[0]["inventory"]["health"] == 0, "death must have HP zero")
            require(deaths[0]["result"]["bossesDefeated"] == 2, "stock boss reward mismatch")
            require(expected[-1]["phase"] == 1 and expected[-1]["inventory"]["maxHealth"] == 100, "retry did not reset run")
        print(f"{name}: {len(steps)} ticks, {len(expected)} checkpoints, {len(outcomes)} outcomes; verified")
    print("Pinned Unity-only source and fixture hashes verified.")


if __name__ == "__main__":
    main()
