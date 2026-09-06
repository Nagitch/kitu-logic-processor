"""Pure result contracts used by the macOS Arena verification coordinator."""

from collections import Counter
from datetime import datetime, timezone
import json
from pathlib import Path
import re
import xml.etree.ElementTree as ET


def utc():
    return datetime.now(timezone.utc).isoformat()


def exact_cases(actual, expected, label):
    """Require one successful execution of every statically declared case."""
    if not expected or len(set(expected)) != len(expected):
        raise ValueError(f"{label}: empty or duplicate expected cases")
    counts = Counter(name for name, _ in actual)
    missing = sorted(set(expected) - counts.keys())
    extra = sorted(counts.keys() - set(expected))
    duplicate = sorted(name for name, count in counts.items() if count != 1)
    failed = [(name, result) for name, result in actual if result != "Passed"]
    if missing or extra or duplicate or failed:
        raise ValueError(f"{label}: missing={missing}, extra={extra}, duplicate={duplicate}, notPassed={failed}")
    return {"total": len(actual), "passed": len(actual), "failed": 0,
            "skipped": 0, "inconclusive": 0, "cases": sorted(counts)}


def nunit_result(path, expected):
    """Check actual testcase identities, results and the independent NUnit totals."""
    root = ET.parse(path).getroot()
    if root.tag != "test-run" or root.get("result") != "Passed":
        raise ValueError("NUnit did not report a passed test-run")
    actual = [(case.get("fullname"), case.get("result")) for case in root.iter("test-case")]
    result = exact_cases(actual, expected, str(path))
    for name in ("total", "passed", "failed", "skipped", "inconclusive"):
        if root.get(name) != str(result[name]):
            raise ValueError(f"NUnit {name} disagrees with actual executed cases")
    return result


def libtest_result(text, expected, label, *, allow_filtered=False):
    """Read stable libtest pretty output from one binary (or one doctest batch)."""
    cases = []
    for line in text.splitlines():
        match = re.fullmatch(r"test (.+) \.\.\. (ok|FAILED|ignored(?:,.*)?)", line)
        if match:
            name = re.sub(r" \(line \d+\)$", "", match[1])
            cases.append((name, "Passed" if match[2] == "ok" else match[2]))
    result = exact_cases(cases, expected, label)
    summaries = re.findall(r"^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out;", text, re.M)
    if (len(summaries) != 1 or summaries[0][:5] != ("ok", str(len(expected)), "0", "0", "0")
            or (not allow_filtered and summaries[0][5] != "0")):
        raise ValueError(f"{label}: libtest summary does not confirm exactly the required cases: {summaries}")
    return result


def cargo_test_binaries(text, expected):
    """Extract actual Cargo artifacts, not guessed hashed test executable paths."""
    binaries = {}
    for line in text.splitlines():
        if not line.startswith("{"):
            continue
        try:
            value = json.loads(line)
        except ValueError:
            continue
        if value.get("reason") != "compiler-artifact" or not value.get("profile", {}).get("test"):
            continue
        target = value["target"]
        if "test" not in target["kind"]:
            continue
        name, executable = target["name"], value.get("executable")
        if name not in expected or not executable or name in binaries:
            raise ValueError(f"Unexpected/duplicate native test artifact: {name}")
        binaries[name] = executable
    if set(binaries) != set(expected):
        raise ValueError(f"Native test artifacts differ: {sorted(binaries)} vs {sorted(expected)}")
    return binaries


def trace_counts(trace, expected, counts, *, forbid_staging=False):
    """Count real ordered trace records and reject a staged initial-package proof."""
    ticks = inputs = 0
    with Path(trace).open() as file:
        for line in file:
            if line.rstrip("\r\n") == "T":
                ticks += 1
            elif line.startswith("I"):
                request = json.loads(line[1:])
                inputs += 1
                if forbid_staging:
                    addresses = [message["address"] for message in request["bundle"]["messages"]]
                    if any(address in ("/input/arena/config", "/input/arena/script", "/input/arena/timeline") for address in addresses):
                        raise ValueError("Initial package trace contains a source staging input")
            else:
                raise ValueError("Malformed native trace")
    with Path(expected).open() as file:
        lines = sum(1 for _ in file)
    if (ticks, inputs) != tuple(counts) or lines != ticks or not ticks:
        raise ValueError(f"Trace scope differs: {(ticks, inputs, lines)}, expected {counts}")
    return {"ticks": ticks, "inputs": inputs}


def completed_scope(steps, cleanup):
    """An unrequested scope is neither a failure nor a completed full proof."""
    required = [step for step in steps if step["required"]]
    return ("passed" if required and all(step["status"] == "passed" for step in required)
            and cleanup["status"] == "passed" else "failed")


def whitespace_only(before, after):
    """Allow only blank-line/trailing-space serialization churn, not YAML values."""
    def normalized(value):
        return [line.rstrip() for line in value.splitlines() if line.strip()]
    return before is not None and after is not None and normalized(before) == normalized(after)
