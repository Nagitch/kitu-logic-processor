#!/usr/bin/env python3
"""Run the same bounded, locked repository checks locally and in CI.

General checks run in the Dev Container. Apple-native and licensed Unity checks
have a separate entry point: verify-arena-macos.py.
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import time

from arena_content import stage_package
from arena_macos import ROOT, artifact, run, write_json

FRONTEND = ROOT / "tools/kitu-web-admin/frontend"
SHARED_ADMIN = ROOT / "tools/kitu-web-admin/package"
ADMIN_STARTER = ROOT / "tools/kitu-web-admin/starter"
SCOPES = ("reference", "fmt", "test", "clippy", "docs", "data", "frontend")


def now():
    return datetime.datetime.now(datetime.timezone.utc).isoformat()


def source_identity():
    def git(*args):
        return subprocess.check_output(["git", *args], cwd=ROOT)
    paths = set(git("diff", "--name-only", "-z", "HEAD").split(b"\0"))
    paths.update(git("ls-files", "--others", "--exclude-standard", "-z").split(b"\0"))
    dirty = []
    for raw in sorted(paths - {b""}):
        relative = os.fsdecode(raw)
        path = ROOT / relative
        dirty.append({"path": relative, "artifact": artifact(path) if path.is_file() else None})
    return {"commit": git("rev-parse", "HEAD").decode().strip(),
            "dirty": bool(git("status", "--porcelain").strip()),
            "dirtyFiles": dirty,
            "locks": [artifact(ROOT / path) for path in
                      ("Cargo.lock", "rust-toolchain.toml",
                       "tools/kitu-web-admin/frontend/pnpm-lock.yaml",
                       "tools/kitu-web-admin/frontend/package.json")]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", choices=("all", *SCOPES), default="all")
    parser.add_argument("--evidence", required=True, type=Path,
                        help="A fresh, absent directory; previous attempts are never overwritten")
    args = parser.parse_args()
    evidence = args.evidence.absolute()
    if evidence.exists() or evidence.is_symlink():
        parser.error("--evidence must be an absent directory")
    evidence.mkdir(parents=True)
    report = {"schemaVersion": 1, "requestedScope": args.scope, "status": "running",
              "environment": {"python": sys.version, "platform": platform.platform(),
                              "pythonBytecodeDisabledForChildren": True},
              "startedAtUtc": now(), "finishedAtUtc": None, "source": None,
              "steps": [], "diagnostic": None}
    destination = evidence / "verification.json"
    write_json(destination, report)

    def command(name, argv, *, cwd=ROOT, extra=None, timeout=3600):
        log = evidence / (name + ".log")
        env = os.environ.copy()
        env["PYTHONDONTWRITEBYTECODE"] = "1"
        env.update(extra or {})
        step = {"id": name, "status": "running", "argv": [str(v) for v in argv],
                "cwd": str(cwd), "environment": extra or {}, "startedAtUtc": now(),
                "finishedAtUtc": None, "timeoutSeconds": timeout, "log": str(log)}
        report["steps"].append(step)
        write_json(destination, report)
        started = time.monotonic()
        try:
            run(argv, cwd=cwd, env=env, timeout=timeout, log=log)
            step.update(status="passed", exitCode=0)
        except BaseException as error:
            step.update(status="failed", diagnostic=str(error) or type(error).__name__)
            raise
        finally:
            step.update(finishedAtUtc=now(), elapsedSeconds=time.monotonic() - started)
            if log.is_file():
                step["logArtifact"] = artifact(log)
            write_json(destination, report)
        return step, log.read_text()

    try:
        report["source"] = source_identity()
        scopes = SCOPES if args.scope == "all" else (args.scope,)
        if set(scopes) - {"reference"}:
            _, version = command("rust-version", ["rustc", "--version"], timeout=60)
            expected = re.search(r'channel\s*=\s*"([^"]+)"',
                                 (ROOT / "rust-toolchain.toml").read_text())[1]
            if not re.search(r"\brustc " + re.escape(expected) + r"\b", version):
                raise RuntimeError(f"Use the pinned Rust {expected} toolchain")
        for scope in scopes:
            if scope in ("test", "clippy", "docs", "data"):
                features = ["--all-features"] if scope in ("clippy", "docs") else []
                command("prepare-" + scope, [sys.executable, ROOT / "tools/prepare-kitu-build.py",
                                            "--manifest-path", ROOT / "apps/demo-game/Cargo.toml",
                                            *features])
            if scope == "reference":
                command(scope, [sys.executable, ROOT / "tools/verify-arena-reference.py"])
            elif scope == "fmt":
                command(scope, ["cargo", "fmt", "--all", "--", "--check"])
            elif scope == "test":
                _, output = command(scope, ["cargo", "test", "--locked", "--workspace"])
                totals = [tuple(map(int, row)) for row in re.findall(
                    r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", output)]
                if not totals or not sum(row[0] for row in totals) or any(row[1] or row[2] for row in totals):
                    raise RuntimeError("Workspace tests must execute and pass without ignored cases")
                report["rustTests"] = {"passed": sum(row[0] for row in totals),
                                       "suites": len(totals), "failed": 0, "ignored": 0}
            elif scope == "clippy":
                command(scope, ["cargo", "clippy", "--locked", "--workspace",
                                "--all-targets", "--all-features", "--", "-D", "warnings"])
            elif scope == "docs":
                command(scope, ["cargo", "doc", "--locked", "--workspace", "--no-deps",
                                "--all-features"], extra={"RUSTDOCFLAGS": "-D warnings"})
            elif scope == "data":
                _, output = command("python-tests", [sys.executable, "-m", "unittest", "discover",
                                                       "-s", "tools/tests", "-p", "test_*.py", "-v"])
                count = re.search(r"Ran (\d+) tests? in", output)
                if not count or int(count[1]) == 0:
                    raise RuntimeError("Portable tool tests did not execute")
                report["pythonTests"] = int(count[1])
                package = stage_package(ROOT / "apps/demo-game/content", evidence / "package")
                write_json(evidence / "package.json", package)
                _, output = command("package-interop", ["cargo", "test", "--locked", "-p",
                    "kitu-demo-game-native", "--test", "package", "--", "--nocapture"],
                    extra={"KITU_PACKAGE_INTEROP_DIR": str(evidence / "package")})
                if "Python package interoperability: " + package["hash"] not in output:
                    raise RuntimeError("Rust/C ABI did not consume the staged Python package")
                report["package"] = package
            elif scope == "frontend":
                _, node = command("node-version", ["node", "--version"], cwd=FRONTEND, timeout=60)
                if not re.search(r"\bv24\.", node):
                    raise RuntimeError("Frontend verification requires Node 24")
                _, pnpm = command("pnpm-version", ["pnpm", "--version"], cwd=FRONTEND, timeout=60)
                pinned = json.loads((FRONTEND / "package.json").read_text())["packageManager"].split("@", 1)[1]
                if pinned not in pnpm.splitlines():
                    raise RuntimeError(f"Frontend verification requires pnpm {pinned}")
                command("shared-admin-install", ["pnpm", "install", "--frozen-lockfile"],
                        cwd=SHARED_ADMIN, extra={"CI": "true"})
                for task in ("build", "check", "test"):
                    command("shared-admin-" + task, ["pnpm", "run", task], cwd=SHARED_ADMIN)
                command("starter-install", ["pnpm", "install", "--frozen-lockfile"],
                        cwd=ADMIN_STARTER, extra={"CI": "true"})
                for task in ("check", "build"):
                    command("starter-" + task, ["pnpm", "run", task], cwd=ADMIN_STARTER)
                command("frontend-install", ["pnpm", "install", "--frozen-lockfile"], cwd=FRONTEND,
                        extra={"CI": "true"})
                for task in ("check", "lint", "test:inspection", "build"):
                    _, output = command("frontend-" + task.replace(":", "-"),
                                        ["pnpm", "run", task], cwd=FRONTEND)
                    if task == "test:inspection":
                        counts = {name: int(value) for name, value in re.findall(
                            r"(?:#|ℹ) (tests|pass|fail|cancelled|skipped|todo) (\d+)", output)}
                        if (not counts.get("tests") or counts.get("pass") != counts["tests"]
                                or any(counts.get(name, -1) != 0 for name in
                                       ("fail", "cancelled", "skipped", "todo"))):
                            raise RuntimeError("Frontend tests must execute without skips or failures")
                        report["frontendTests"] = counts
                report["frontendIncludesWasmPrebuild"] = True
        report["status"] = "passed"
    except BaseException as error:
        report["status"] = "failed"
        report["diagnostic"] = str(error) or type(error).__name__
        raise
    finally:
        report["finishedAtUtc"] = now()
        write_json(destination, report)
        print(json.dumps({"status": report["status"], "report": str(destination)}), flush=True)


if __name__ == "__main__":
    main()
