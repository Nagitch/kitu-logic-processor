#!/usr/bin/env python3
"""Run declared native or full Arena proofs and retain a fresh, honest report.

Native scope needs Apple Silicon, the Apple SDK and the pinned Rust toolchain.
Full additionally needs the project's licensed Unity Editor and a graphical login.
No source/game implementation, remote host or historical recording is substituted.
"""

import argparse
from contextlib import contextmanager
import fcntl
import json
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import sys
import tempfile
import time
import urllib.request

from arena_content import FILES, inspect_package, stage_package
from arena_macos import (LIBRARY, PLUGIN, PROJECT, ROOT, OwnedProcess, artifact, clone_tree,
                         require_closed_editor, require_macos, run, sha256,
                         termination_guard, verify_plugin, write_json)
from arena_verification import (cargo_test_binaries, completed_scope, libtest_result,
                                nunit_result, report_environment, trace_counts, utc, whitespace_only)

TARGET = "aarch64-apple-darwin"
TOOLS = ROOT / "tools"
CASES = TOOLS / "arena-verification-cases.json"
STOCK_TEST = "real_tsq1_replays_every_stock_tick_state_and_event_through_eleven_death_retry"
READBACK_TEST = "application::tests::csharp_reencoded_frames_decode_to_identical_typed_values_and_float_bits"
TRACES = {"preparation": (28, 40), "stock-eleven-death-retry": (5528, 5581),
          "rhai-boss": (1800, 1817), "timeline-cues": (1830, 1819)}
NATIVE_STEPS = ("preflight", "source-package", "native-tests", "native-build", "c-abi")
FULL_STEPS = ("default-player-build", "server-provision", "unity-edit", "unity-msgpack",
              "unity-json", "codec-readback", "inspection-identities", "server-stop",
              "default-player-traces", "default-content-probes", "edited-player-build",
              "edited-player-trace", "edited-content-probe")
SETTINGS = ["ProjectSettings/" + name for name in (
    "EditorBuildSettings.asset", "GraphicsSettings.asset", "QualitySettings.asset",
    "ProjectSettings.asset", "ProjectAuditorSettings.asset", "UnityConnectSettings.asset")]
ADDRESSABLE_SETTINGS = "Assets/AddressableAssetsData/AddressableAssetSettings.asset"
GENERATED_LINK_FILES = ["Assets/AddressableAssetsData/link.xml",
                        "Assets/AddressableAssetsData/link.xml.meta"]


def json_output(text):
    """Read one JSON result after the command/log prefix, never eval shell output."""
    decoder = json.JSONDecoder()
    lines = text.splitlines(keepends=True)
    for index, line in enumerate(lines):
        if line.startswith(("{", "[")):
            try:
                result, end = decoder.raw_decode("".join(lines[index:]))
                if "".join(lines[index:])[end:].strip():
                    continue
                return result
            except ValueError:
                pass
    raise ValueError("Command did not emit one complete JSON result")


def bytes_or_none(path):
    if path.is_symlink():
        raise ValueError(f"Refusing settings symlink: {path}")
    return path.read_bytes() if path.exists() else None


class ProjectGuard:
    """Restore only captured build-owned settings; detect other source changes."""

    def __init__(self, project=PROJECT, tracked=None):
        self.project = project
        if tracked is None:
            prefix = str(project.relative_to(ROOT)) + "/"
            files = run(["git", "ls-files", "-z", "--", prefix], cwd=ROOT).split("\0")
            tracked = [Path(file).relative_to(project.relative_to(ROOT)).as_posix()
                       for file in files if file]
        self.owned = set(SETTINGS + [ADDRESSABLE_SETTINGS] + GENERATED_LINK_FILES)
        self.paths = sorted(set(tracked) | self.owned)
        self.initial = {name: bytes_or_none(project / name) for name in self.paths}
        self.last = self.initial.copy()
        self.unexpected = set()

    def verify_idle(self):
        for name in self.paths:
            if bytes_or_none(self.project / name) != self.last[name]:
                raise RuntimeError(f"Project changed outside the owned Editor operation: {name}")

    def capture_editor(self):
        for name in self.paths:
            value = bytes_or_none(self.project / name)
            if value != self.last[name] and (name not in self.owned or (
                    name == ADDRESSABLE_SETTINGS and not whitespace_only(self.last[name], value))):
                self.unexpected.add(name)
            self.last[name] = value
        if self.unexpected:
            raise RuntimeError(f"Unexpected tracked Editor changes: {sorted(self.unexpected)}")

    def restore(self):
        restored, conflicts = [], sorted(self.unexpected)
        for name in self.owned:
            current, initial = bytes_or_none(self.project / name), self.initial[name]
            if current == initial:
                continue
            if name in self.unexpected or current != self.last[name]:
                conflicts.append(name)
                continue
            path = self.project / name
            if initial is None:
                path.unlink(missing_ok=True)
            else:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(initial)
            restored.append(name)
        for name in self.paths:
            if name not in self.owned and bytes_or_none(self.project / name) != self.initial[name]:
                conflicts.append(name)
        return {"restoredFiles": sorted(restored), "unexpectedChanges": sorted(set(conflicts))}


class Verification:
    def __init__(self, args):
        self.args = args
        if args.evidence.is_symlink():
            raise ValueError("Evidence must not be a symlink")
        self.evidence = args.evidence.resolve()
        self.evidence.mkdir(parents=True, exist_ok=False)
        self.env = {key: value for key, value in os.environ.items()
                    if not key.startswith(("KITU_ARENA_", "KITU_NATIVE_", "KITU_PACKAGED_", "KITU_WIRE_", "KITU_PACKAGE_INTEROP", "KITU_REPLAY_EVIDENCE", "KITU_APPLICATION_WIRE_"))}
        self.env.update(CARGO_INCREMENTAL="0", CARGO_PROFILE_DEV_DEBUG="0", CARGO_PROFILE_TEST_DEBUG="0")
        self.env["PYTHONDONTWRITEBYTECODE"] = "1"
        self.cases = json.loads(CASES.read_text())
        if self.cases.get("schemaVersion") != 1:
            raise ValueError("Unknown verification case manifest")
        self.report = {"schemaVersion": 1, "requestedScope": args.scope, "status": "running",
                       "startedAtUtc": utc(), "finishedAtUtc": None, "source": None,
                       "toolchain": {"target": TARGET, "profile": args.profile,
                                     "python": sys.version, "platform": platform.platform(),
                                     "macOS": platform.mac_ver()[0]}, "execution": None,
                       "steps": [{"id": name, "required": name in NATIVE_STEPS or args.scope == "full",
                                  "status": "notRun" if name in NATIVE_STEPS or args.scope == "full" else "notRequested",
                                  "commands": [], "artifacts": [], "summary": None, "diagnostic": None}
                                 for name in NATIVE_STEPS + FULL_STEPS],
                       "cleanup": {"status": "notRun", "ownedProcesses": [], "restoredFiles": [], "unexpectedChanges": []}}
        self.step = None
        self.command_id = 0
        self.host = None
        self.project_guard = None
        self.relocated = None
        self.write()

    def write(self):
        write_json(self.evidence / "verification.json", self.report)

    def do(self, name, function):
        self.step = next(step for step in self.report["steps"] if step["id"] == name)
        self.step.update(status="running", startedAtUtc=utc())
        self.write()
        print(f"Arena verification: {name}", flush=True)
        try:
            self.step["summary"] = function()
            self.step["status"] = "passed"
        except BaseException as error:
            self.step.update(status="failed", diagnostic=str(error) or type(error).__name__)
            raise
        finally:
            self.step["finishedAtUtc"] = utc()
            self.write()

    def command(self, arguments, *, env=None, timeout=1800, cwd=ROOT):
        self.command_id += 1
        log = self.evidence / "commands" / f"{self.command_id:03d}-{self.step['id']}.log"
        selected = env or self.env
        item = {"argv": [str(a) for a in arguments], "cwd": str(cwd), "startedAtUtc": utc(),
                "finishedAtUtc": None, "exit": None, "timeoutSeconds": timeout, "log": str(log),
                "environment": report_environment(selected)}
        self.step["commands"].append(item)
        self.write()
        try:
            # Wrapper children need their own ten-second cleanup before our final kill.
            with OwnedProcess(arguments, log=log, env=selected, cwd=cwd, shutdown_grace=30) as owned:
                try:
                    status = owned.wait(timeout)
                finally:
                    item["pid"] = owned.process.pid
                    item["exit"] = owned.process.returncode
                if status:
                    raise RuntimeError(f"Command exited {status}: {log}")
            return log.read_text(errors="replace")
        finally:
            item["finishedAtUtc"] = utc()
            if log.exists():
                item["logArtifact"] = artifact(log)
            self.write()

    def tool(self, name, *arguments, timeout=1800, env=None):
        return self.command([sys.executable, TOOLS / name, *arguments], timeout=timeout, env=env)

    def retain(self, path):
        value = artifact(path)
        self.step["artifacts"].append(value)
        return value

    def cargo(self, verb, *arguments, timeout=1800, env=None):
        self.command([sys.executable, TOOLS / "prepare-kitu-build.py", "--manifest-path",
                      ROOT / "apps/demo-game/Cargo.toml", "--cargo", self.args.cargo,
                      "--target", TARGET], env=env)
        return self.command([self.args.cargo, verb, "--locked", "--target", TARGET,
                             "--profile", self.args.profile, *arguments], timeout=timeout, env=env)

    def preflight(self):
        require_macos()
        require_closed_editor()
        rustc = self.env.get("RUSTC", "rustc")
        toolchain = self.report["toolchain"]
        for key, command in (("cargo", [self.args.cargo, "--version"]),
                             ("rustc", [rustc, "--version", "--verbose"]),
                             ("sdk", ["xcrun", "--show-sdk-version"]),
                             ("clang", ["xcrun", "--find", "clang"])):
            toolchain[key] = self.command(command, timeout=30).splitlines()[1:]
        expected = re.search(r'channel\s*=\s*"([^"]+)"', (ROOT / "rust-toolchain.toml").read_text())[1]
        if not any(line.startswith("rustc " + expected + " ") for line in toolchain["rustc"]):
            raise RuntimeError(f"Rust must match rust-toolchain.toml {expected}")
        if not any(line.startswith("cargo " + expected + " ") for line in toolchain["cargo"]):
            raise RuntimeError(f"Cargo must match rust-toolchain.toml {expected}")
        metadata = json_output(self.command([self.args.cargo, "metadata", "--locked", "--no-deps", "--format-version", "1"]))
        profile = "debug" if self.args.profile == "dev" else "release"
        self.target = Path(metadata["target_directory"]) / TARGET / profile
        dirty_paths = set(run(["git", "diff", "--name-only", "-z", "HEAD"], cwd=ROOT).split("\0"))
        dirty_paths.update(run(["git", "ls-files", "--others", "--exclude-standard", "-z"], cwd=ROOT).split("\0"))
        self.report["source"] = {"commit": run(["git", "rev-parse", "HEAD"], cwd=ROOT),
                                 "dirty": bool(run(["git", "status", "--porcelain"], cwd=ROOT)),
                                 "dirtyFiles": [{"path": name, "artifact": artifact(ROOT / name) if (ROOT / name).is_file() else None}
                                                for name in sorted(dirty_paths - {""})],
                                 "inputs": [artifact(ROOT / path) for path in ("Cargo.lock", "rust-toolchain.toml", "tools/arena-verification-cases.json",
                                    "tools/kitu-web-admin/frontend/pnpm-lock.yaml",
                                    "tools/kitu-web-admin/frontend/package.json",
                                    str(PROJECT.relative_to(ROOT) / "Packages/manifest.json"),
                                    str(PROJECT.relative_to(ROOT) / "Packages/packages-lock.json"),
                                    str(PROJECT.relative_to(ROOT) / "ProjectSettings/ProjectVersion.txt"))]}
        if self.args.scope == "full":
            version = next(line.split(": ", 1)[1] for line in (PROJECT / "ProjectSettings/ProjectVersion.txt").read_text().splitlines() if line.startswith("m_EditorVersion: "))
            self.editor = self.args.editor or Path(f"/Applications/Unity/Hub/Editor/{version}/Unity.app/Contents/MacOS/Unity")
            if not self.editor.is_file():
                raise RuntimeError(f"Full scope requires the licensed Unity {version} Editor: {self.editor}")
            toolchain.update(unity=version, editor=str(self.editor))
            self.project_guard = ProjectGuard()
            self.port = self.args.port or self.available_port()
            with socket.socket() as probe:
                probe.bind(("127.0.0.1", self.port))
        return {"targetDirectory": str(self.target), "scope": self.args.scope}

    @staticmethod
    def available_port():
        with socket.socket() as probe:
            probe.bind(("127.0.0.1", 0))
            return probe.getsockname()[1]

    def package(self):
        result = self.evidence / "package.json"
        self.tool("package-arena-content.py", "--source", ROOT / "apps/demo-game/content",
                  "--destination", self.evidence / "default-package", "--evidence", result)
        self.retain(result)
        return inspect_package(self.evidence / "default-package")

    def native_tests(self):
        env = self.env | {"KITU_NATIVE_EVIDENCE_DIR": str(self.evidence / "native-traces"),
                          "KITU_PACKAGED_PLAYER_EVIDENCE_DIR": str(self.evidence / "packaged-oracle"),
                          "KITU_WIRE_EVIDENCE_DIR": str(self.evidence / "wire-parity"),
                          "KITU_PACKAGE_INTEROP_DIR": str(self.evidence / "default-package")}
        built = self.cargo("test", "-p", "kitu-demo-game-native", "--tests", "--no-run", "--message-format=json", env=env)
        binaries = cargo_test_binaries(built, self.cases["native"])
        summaries = {}
        for name, path in sorted(binaries.items()):
            output = self.command([path, "--test-threads=1", "--show-output"], env=env, timeout=3600)
            summaries[name] = libtest_result(output, self.cases["native"][name], name)
            if name == "package" and "Python package interoperability: " + inspect_package(self.evidence / "default-package")["hash"] not in output:
                raise ValueError("Native test did not consume the actual Python package")
        output = self.cargo("test", "-p", "kitu-demo-game-native", "--doc", env=env)
        summaries["doctests"] = libtest_result(output, self.cases["nativeDoctests"], "native doctests")
        for name, counts in TRACES.items():
            self.validate_trace(name, self.evidence / "native-traces", counts)
        self.validate_trace("bundled-edited", self.evidence / "packaged-oracle", (1800, 1816), True)
        for path in sorted((self.evidence / "packaged-oracle").glob("*.json")):
            self.retain(path)
        for name in ("preparation", "stock-eleven-death-retry", "edited-rhai-timeline", "wide-identities"):
            path = self.evidence / "wire-parity" / (name + ".json")
            proof = json.loads(path.read_text())
            if proof.get("matched") is not True or proof.get("execution", {}).get("target") != TARGET:
                raise ValueError("Native socket/C ABI proof lacks its actual target execution")
            if self.report["execution"] is None:
                self.report["execution"] = proof["execution"]
            elif self.report["execution"] != proof["execution"]:
                raise ValueError("Native scenario execution identities differ")
            self.retain(path)
        return summaries

    def validate_trace(self, name, directory, counts, packaged=False):
        trace, expected = directory / (name + ".trace"), directory / (name + ".expected.ndjson")
        result = trace_counts(trace, expected, counts, forbid_staging=packaged)
        self.retain(trace)
        self.retain(expected)
        return result

    def native_build(self):
        self.tool("build-arena-native-macos.py", "--cargo", self.args.cargo,
                  "--profile", self.args.profile, "--evidence", self.evidence / "native-build")
        report = self.evidence / "native-build/native-package.json"
        self.retain(report)
        saved = self.evidence / "native-build" / LIBRARY
        shutil.copy2(PLUGIN, saved)
        self.retain(saved)
        return verify_plugin(saved)

    def c_abi(self):
        binary = self.evidence / "c-abi-caller"
        self.command(["xcrun", "clang", "-std=c11", "-Wall", "-Wextra", "-Werror",
                      "-I", ROOT / "crates/kitu-unity-ffi/include",
                      ROOT / "apps/demo-game/native/tests/c_abi.c", "-L", self.evidence / "native-build",
                      "-lkitu_demo_game_native", "-Wl,-rpath," + str(self.evidence / "native-build"), "-o", binary])
        self.retain(binary)
        summaries = {}
        directory = self.evidence / "c-output"
        directory.mkdir()
        for name in TRACES:
            actual = directory / (name + ".ndjson")
            self.command([binary, self.evidence / "native-traces" / (name + ".trace"), actual], timeout=600)
            checked = json_output(self.tool("verify-arena-native.py", self.evidence / "native-traces" / (name + ".expected.ndjson"), actual))
            if checked["ticks"] != TRACES[name][0]:
                raise ValueError("C comparison covered the wrong scenario")
            summaries[name] = checked
            self.retain(actual)
        return summaries

    @contextmanager
    def editor_operation(self):
        require_closed_editor()
        self.project_guard.verify_idle()
        try:
            yield
        finally:
            require_closed_editor()
            self.project_guard.capture_editor()

    def player_build(self, edited=False):
        name = "edited" if edited else "default"
        source = self.evidence / ("packaged-oracle/edited-package" if edited else "default-package")
        player = self.evidence / "build" / (name + ".app")
        output = self.evidence / (name + "-build")
        with self.editor_operation():
            self.tool("build-arena-player-macos.py", "--editor", self.editor, "--player", player,
                      "--evidence", output, "--content-source", source, timeout=1860)
        report = json.loads((output / "player-build.json").read_text())
        if edited and report["bundledPlugin"]["sha256"] != self.default_plugin_hash:
            raise ValueError("Edited package Player changed the native dylib bytes")
        if not edited:
            self.default_plugin_hash = report["bundledPlugin"]["sha256"]
        self.retain(output / "player-build.json")
        return report

    def http(self, route, data=None, content_type=None):
        request = urllib.request.Request(self.endpoint + route, data=data)
        if content_type:
            request.add_header("Content-Type", content_type)
        with urllib.request.urlopen(request, timeout=30) as response:
            raw = response.read(8 * 1024 * 1024 + 1)
            if len(raw) > 8 * 1024 * 1024:
                raise ValueError("Verification HTTP response exceeded8MiB")
            return json.loads(raw)

    def cli(self, *arguments):
        return json_output(self.command([self.target / "kitu-cli", "--endpoint", self.endpoint, *arguments], timeout=60))

    def provision(self):
        self.cargo("build", "-p", "kitu-demo-game", "--bin", "kitu-demo-game-admin-host")
        self.cargo("build", "-p", "kitu-cli", "--bin", "kitu-cli")
        env = self.env | {"KITU_REPLAY_EVIDENCE_DIR": str(self.evidence / "replay")}
        text = self.cargo("test", "-p", "kitu-demo-game", "--test", "arena_replay", STOCK_TEST, "--", "--exact", env=env, timeout=1800)
        libtest_result(text, [STOCK_TEST], "stock recording", allow_filtered=True)
        recording = self.evidence / "replay/stock-eleven-death-retry.tsq"
        verification = self.evidence / "replay/stock-verification.json"
        self.retain(recording)
        self.retain(verification)
        authoring = self.evidence / "server/authoring"
        shutil.copytree(self.evidence / "default-package", authoring)
        env = self.env | {"KITU_DEMO_GAME_BIND": f"127.0.0.1:{self.port}", "RUST_LOG": "info",
                          "KITU_ARENA_CONTENT": str(authoring / "arena.tmd"),
                          "KITU_ARENA_SCRIPT": str(authoring / "boss.rhai"),
                          "KITU_ARENA_TIMELINE_DIRECTORY": str(authoring / "timelines"),
                          "KITU_ARENA_RUN_DIRECTORY": str(self.evidence / "server/runs"),
                          "KITU_ARENA_RECORDING_DIRECTORY": str(self.evidence / "server/recordings")}
        self.endpoint = f"http://127.0.0.1:{self.port}"
        self.host = OwnedProcess([self.target / "kitu-demo-game-admin-host"], log=self.evidence / "server/host.log", env=env, cwd=ROOT)
        self.report["cleanup"]["ownedProcesses"].append({"pid": self.host.process.pid, "role": "server", "endpoint": self.endpoint})
        deadline = time.monotonic() + 30
        while time.monotonic() < deadline:
            if self.host.process.poll() is not None:
                raise RuntimeError("Owned server exited before readiness; inspect server/host.log")
            if "admin host listening on" in (self.evidence / "server/host.log").read_text(errors="replace"):
                break
            time.sleep(.1)
        else:
            raise RuntimeError("Owned server did not report readiness within30seconds")
        listener = self.command(["/usr/sbin/lsof", "-nP", "-a", "-p", str(self.host.process.pid),
                                 "-iTCP:" + str(self.port), "-sTCP:LISTEN", "-F", "p"], timeout=10)
        if f"p{self.host.process.pid}" not in listener.splitlines():
            raise ValueError("Selected endpoint does not belong to the owned host")
        health = self.http("/health")
        if health != {"status": "ok", "service": "kitu-demo-game-admin-host"}:
            raise ValueError("Owned host health did not identify the Arena service")
        imported = self.http("/arena/recordings/import", recording.read_bytes(), "application/octet-stream")
        self.recording_id = imported["id"]
        if self.recording_id != sha256(recording):
            raise ValueError("Import ID does not match the actual recording bytes")
        checked = self.cli("replay", "verify", self.recording_id)
        if checked.get("ok") is not True or any(checked.get("data", {}).get(k) != n for k, n in (("ticks", 5528), ("inputs", 5581), ("runs", 2))):
            raise ValueError(f"Actual CLI did not verify the fresh stock recording: {checked}")
        inspection = self.http("/arena/inspection")
        if self.report["execution"] != inspection["execution"]:
            raise ValueError("Owned server execution differs from the actual native proof")
        self.server_session = inspection["sessionId"]
        result = {"endpoint": self.endpoint, "pid": self.host.process.pid,
                  "sessionId": self.server_session, "import": imported, "cliVerification": checked,
                  "execution": inspection["execution"],
                  "host": artifact(self.target / "kitu-demo-game-admin-host"), "cli": artifact(self.target / "kitu-cli")}
        write_json(self.evidence / "server/provision.json", result)
        self.retain(self.evidence / "server/provision.json")
        return result

    def unity(self, mode):
        directory = self.evidence / "unity"
        directory.mkdir(exist_ok=True)
        env = self.env | {"KITU_ARENA_WS_URL": self.endpoint.replace("http://", "ws://") + "/ws/arena",
                          "KITU_ARENA_REPLAY_ID": self.recording_id,
                          "KITU_ARENA_CLI_EXECUTABLE": str(self.target / "kitu-cli"),
                          "KITU_ARENA_CLI_ARGUMENTS": "--endpoint " + self.endpoint,
                          "KITU_ARENA_EXPECTED_STARTER_DAMAGE": "20",
                          "KITU_ARENA_ENCODING": "json" if mode == "json" else "msgpack",
                          "KITU_ARENA_INSPECTION_EVIDENCE_DIR": str(directory / (mode + "-inspection")),
                          "KITU_ARENA_WIRE_EVIDENCE_DIR": str(directory / "wire")}
        xml = directory / (mode + ".xml")
        command = [self.editor, "-batchmode", "-projectPath", PROJECT, "-runTests", "-testPlatform",
                   "EditMode" if mode == "edit" else "PlayMode",
                   "-testResults", xml, "-logFile", directory / (mode + ".log")]
        if mode != "edit":
            command += ["-testFilter", "UnityOnlyArena.Tests"]
        if mode == "json":
            command += ["-testCategory", "ArenaNetwork"]
        with self.editor_operation():
            self.command(command, env=env, timeout=2700)
        name = {"edit": "unityEdit", "msgpack": "unityPlayMsgpack", "json": "unityPlayJson"}[mode]
        result = nunit_result(xml, self.cases[name])
        self.retain(xml)
        return result

    def codec_readback(self):
        corpus = json.loads((ROOT / "crates/kitu-transport/tests/fixtures/application-wire/manifest.json").read_text())
        directory = self.evidence / "unity/wire"
        valid = [case for case in corpus["cases"] if case["valid"]]
        for case in valid:
            for encoding in ("json", "msgpack"):
                path = directory / case[encoding]
                if not path.is_file() or not path.stat().st_size:
                    raise ValueError(f"Missing actual Unity codec export: {path}")
                self.retain(path)
        text = self.cargo("test", "-p", "kitu-transport", "--lib", READBACK_TEST, "--", "--exact",
                          env=self.env | {"KITU_APPLICATION_WIRE_CSHARP_FIXTURES": str(directory)})
        result = libtest_result(text, [READBACK_TEST], "C# readback", allow_filtered=True)
        return result | {"pairs": len(valid)}

    def inspection_identities(self):
        paths = [self.evidence / "unity/msgpack-inspection/native-inspection.json",
                 self.evidence / "unity/msgpack-inspection/network-inspection-msgpack.json",
                 self.evidence / "unity/json-inspection/network-inspection-json.json"]
        results = []
        for path in paths:
            captures = json.loads(path.read_text())
            if not isinstance(captures, list) or not captures:
                raise ValueError("No actual Unity inspection captures")
            for capture in captures:
                if capture["inspection"]["execution"] != self.report["execution"]:
                    raise ValueError("Native/Unity/network inspection execution identity differs")
            results.append({"artifact": self.retain(path), "captures": len(captures)})
        return {"execution": self.report["execution"], "captures": results}

    def stop_server(self):
        if not self.host:
            return {"stopped": False}
        status = self.host.stop()
        pid = self.host.process.pid
        self.host = None
        self.report["cleanup"]["ownedProcesses"][-1]["exit"] = status
        try:
            with urllib.request.urlopen(self.endpoint + "/arena/inspection", timeout=1) as response:
                current = json.loads(response.read(8 * 1024 * 1024))
            if current.get("sessionId") == self.server_session:
                raise RuntimeError("Owned host still serves after process cleanup")
        except (OSError, TimeoutError):
            pass
        return {"stopped": True, "pid": pid, "exit": status, "ownedEndpointStoppedBeforeGraphicalProof": True}

    def player_traces(self, edited=False):
        result = {}
        scenarios = {"bundled-edited": (1800, 1816)} if edited else TRACES
        source = self.evidence / ("packaged-oracle" if edited else "native-traces")
        for name, counts in scenarios.items():
            self.validate_trace(name, source, counts, edited)
            evidence = self.evidence / ("player-" + name)
            extras = []
            if edited:
                initial = source / (name + ".initial.json")
                if not initial.is_file() or not initial.stat().st_size:
                    raise RuntimeError(f"Edited package proof requires its initial oracle: {initial}")
                self.retain(initial)
                extras = ["--", "--arena-initial-expected", initial]
            self.tool("run-arena-player-verification.py", "--player", self.evidence / "build" / ("edited.app" if edited else "default.app"),
                      "--trace", source / (name + ".trace"), "--expected", source / (name + ".expected.ndjson"),
                      "--evidence", evidence, *extras, timeout=660)
            result[name] = self.retain(evidence / "player-verification.json")
        return result

    def content_probe(self, name, player, *, package=None, failure=None, storage=None, relocate=False, identity=None):
        evidence = self.evidence / ("content-" + name)
        args = ["--player", player, "--evidence", evidence, "--expected-native-sha256", self.default_plugin_hash]
        if package:
            args += ["--package", package]
        if storage:
            args += ["--storage", storage]
        if failure:
            args += ["--expect-failure", failure]
        if identity:
            args += ["--expected-package-hash", identity]
        if relocate:
            args += ["--relocate-to", self.relocated / (name + ".app")]
        self.tool("verify-arena-packaged-player.py", *args, timeout=180)
        report = evidence / "content-player-verification.json"
        return {"artifact": self.retain(report), "report": json.loads(report.read_text())}

    def content_probes(self):
        self.relocated = Path(tempfile.mkdtemp(prefix="kitu-arena-verification-"))
        self.report["relocatedArtifactsDirectory"] = str(self.relocated)
        default = self.evidence / "build/default.app"
        identity = inspect_package(self.evidence / "default-package")["hash"]
        positive = self.content_probe("relocated", default, relocate=True, identity=identity)
        app = Path(positive["report"]["player"])
        storage = Path(positive["report"]["authoringStorage"])
        # A valid authoring edit is deliberately independent of immutable initial package selection.
        with (storage / "boss.rhai").open("a") as file:
            file.write("\n// retained authoring edit for no-overwrite verification\n")
        repeated = self.content_probe("no-overwrite", app, storage=storage, identity=identity)
        reports = {"relocated": positive["artifact"], "noOverwrite": repeated["artifact"]}
        for name in ("missing-key", "wrong-type", "digest", "schema", "invalid-script"):
            directory = self.evidence / "negative-packages" / name
            shutil.copytree(self.evidence / "default-package", directory)
            if name in ("missing-key", "wrong-type"):
                mapping = json.loads((directory / "unity-assets.json").read_text())
                if name == "missing-key":
                    mapping["assets"][0]["key"] = "arena/missing/verification"
                else:
                    entries = mapping["assets"]
                    entries[0]["key"], entries[1]["key"] = entries[1]["key"], entries[0]["key"]
                write_json(directory / "unity-assets.json", mapping)
                self.rehash(directory)
                failure = "Cannot load Arena Addressable"
            elif name == "digest":
                with (directory / "arena.tmd").open("a") as file:
                    file.write("\n")
                failure = "digest mismatch"
            elif name == "schema":
                manifest = json.loads((directory / "package.json").read_text())
                manifest["schemaVersion"] = 999
                write_json(directory / "package.json", manifest)
                failure = "Incompatible Arena package schema"
            else:
                (directory / "boss.rhai").write_text("fn boss( broken syntax")
                self.rehash(directory)
                failure = "invalid packaged Rhai"
            reports[name] = self.content_probe(name, app, package=directory, failure=failure)["artifact"]
        missing = self.relocated / "missing-bundle.app"
        clone_tree(app, missing)
        bundles = [missing / Path(item["path"]).relative_to(app)
                   for item in positive["report"]["independentChecks"]["runtimeBundles"]]
        if not bundles:
            raise ValueError("Built app has no bundle for the negative Player probe")
        for bundle in bundles:
            bundle.unlink()
        self.command(["/usr/bin/codesign", "--force", "--sign", "-", "--timestamp=none", missing])
        reports["missing-bundle"] = self.content_probe("missing-bundle", missing, failure="Cannot load Arena Addressable")["artifact"]
        return reports

    @staticmethod
    def rehash(directory):
        # Delegate fixed schema/digests to the production packager, using a separate source.
        staged = directory.with_name(directory.name + "-staged")
        stage_package(directory, staged)
        shutil.copy2(staged / "package.json", directory / "package.json")

    def edited_probe(self):
        if self.relocated is None:
            raise RuntimeError("Default relocated proof did not run")
        identity = inspect_package(self.evidence / "packaged-oracle/edited-package")["hash"]
        return self.content_probe("edited-relocated", self.evidence / "build/edited.app", relocate=True, identity=identity)["artifact"]

    def execute(self):
        failure = None
        try:
            with termination_guard():
                self.do("preflight", self.preflight)
                self.do("source-package", self.package)
                self.do("native-tests", self.native_tests)
                self.do("native-build", self.native_build)
                self.do("c-abi", self.c_abi)
                if self.args.scope == "full":
                    self.do("default-player-build", self.player_build)
                    self.do("server-provision", self.provision)
                    self.do("unity-edit", lambda: self.unity("edit"))
                    self.do("unity-msgpack", lambda: self.unity("msgpack"))
                    self.do("unity-json", lambda: self.unity("json"))
                    self.do("codec-readback", self.codec_readback)
                    self.do("inspection-identities", self.inspection_identities)
                    self.do("server-stop", self.stop_server)
                    self.do("default-player-traces", self.player_traces)
                    self.do("default-content-probes", self.content_probes)
                    self.do("edited-player-build", lambda: self.player_build(True))
                    self.do("edited-player-trace", lambda: self.player_traces(True))
                    self.do("edited-content-probe", self.edited_probe)
        except BaseException as error:
            failure = error
            self.report["diagnostic"] = str(error) or type(error).__name__
        finally:
            cleanup = self.report["cleanup"]
            try:
                if self.host:
                    self.stop_server()
                if self.project_guard:
                    require_closed_editor()
                    cleanup.update(self.project_guard.restore())
                cleanup["status"] = "failed" if cleanup["unexpectedChanges"] else "passed"
            except BaseException as error:
                cleanup.update(status="failed", diagnostic=str(error))
            self.report["status"] = completed_scope(self.report["steps"], cleanup)
            self.report["finishedAtUtc"] = utc()
            self.write()
        print(json.dumps({"status": self.report["status"], "scope": self.args.scope,
                          "report": str(self.evidence / "verification.json")}))
        if failure:
            print(str(failure), file=sys.stderr)
        return 0 if self.report["status"] == "passed" else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--scope", choices=("native", "full"), required=True)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    parser.add_argument("--profile", choices=("dev", "release"), default="dev")
    parser.add_argument("--editor", type=Path)
    parser.add_argument("--port", type=int)
    args = parser.parse_args()
    if args.scope == "native" and (args.editor is not None or args.port is not None):
        parser.error("--editor/--port require --scope full")
    if args.port is not None and not 1 <= args.port <= 65535:
        parser.error("--port must be1..65535")
    if args.evidence.exists() or args.evidence.is_symlink():
        parser.error("--evidence must be an absent attempt directory")
    lock = ROOT / ".tmp/arena-macos-verification.lock"
    lock.parent.mkdir(exist_ok=True)
    with lock.open("a") as file:
        try:
            fcntl.flock(file, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            parser.error("Another Arena macOS verification owns this checkout")
        return Verification(args).execute()


if __name__ == "__main__":
    sys.exit(main())
