"""Portable verification-contract tests; no Cargo, Unity, server or browser starts."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
from arena_macos import OwnedProcess, clone_tree
from arena_verification import (cargo_test_binaries, completed_scope, libtest_result,
                                nunit_result, trace_counts, whitespace_only)
spec = importlib.util.spec_from_file_location("arena_coordinator", TOOLS / "verify-arena-macos.py")
coordinator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(coordinator)


class VerificationContracts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)

    def tearDown(self):
        self.temporary.cleanup()

    def xml(self, cases, **attributes):
        from xml.etree.ElementTree import Element, SubElement, ElementTree
        root = Element("test-run", {"result": "Passed", "total": str(len(cases)), "passed": str(len(cases)),
                                    "failed": "0", "skipped": "0", "inconclusive": "0", **attributes})
        for name, result in cases:
            SubElement(root, "test-case", {"fullname": name, "result": result})
        path = self.directory / "tests.xml"
        ElementTree(root).write(path)
        return path

    def test_nunit_counts_do_not_replace_exact_names_and_success(self):
        self.assertEqual(nunit_result(self.xml([("a", "Passed")]), ["a"])["passed"], 1)
        for cases in ([("other", "Passed")], [("a", "Skipped")], [("a", "Inconclusive")],
                      [("a", "Failed")], [("a", "Passed"), ("a", "Passed")], []):
            with self.subTest(cases=cases), self.assertRaises(ValueError):
                nunit_result(self.xml(cases), ["a"])
        with self.assertRaises(ValueError):
            nunit_result(self.xml([("a", "Passed")], total="143"), ["a"])
        with self.assertRaises(ValueError):
            nunit_result(self.xml([("a", "Passed")], result="Failed"), ["a"])

    def test_libtest_checks_independent_summary_and_filtered_scope(self):
        text = "test a ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in0s\n"
        self.assertEqual(libtest_result(text, ["a"], "test")["passed"], 1)
        for malformed in (text.replace("a ...", "b ..."), text.replace("... ok", "... ignored"),
                          text.replace("1 passed", "0 passed"), text + text,
                          text.replace("0 filtered", "4 filtered")):
            with self.subTest(text=malformed), self.assertRaises(ValueError):
                libtest_result(malformed, ["a"], "test")
        self.assertEqual(libtest_result(text.replace("0 filtered", "4 filtered"), ["a"], "test", allow_filtered=True)["passed"], 1)
        self.assertEqual(libtest_result(text.replace("test a", "test a (line 141)"), ["a"], "doc")["passed"], 1)

    def test_cargo_artifacts_require_the_declared_actual_integration_binaries(self):
        value = {"reason": "compiler-artifact", "profile": {"test": True},
                 "target": {"kind": ["test"], "name": "native"}, "executable": "/tmp/actual-hash"}
        text = json.dumps(value)
        self.assertEqual(cargo_test_binaries(text, {"native": []}), {"native": "/tmp/actual-hash"})
        for output in ("", text + "\n" + text, text.replace('"native"', '"other"')):
            with self.assertRaises(ValueError):
                cargo_test_binaries(output, {"native": []})

    def test_initial_package_trace_cannot_smuggle_a_staging_input(self):
        trace, expected = self.directory / "scenario.trace", self.directory / "expected.ndjson"
        expected.write_text('{}\n')
        trace.write_text('I{"bundle":{"messages":[{"address":"/input/arena/start"}]}}\nT\n')
        self.assertEqual(trace_counts(trace, expected, (1, 1), forbid_staging=True), {"ticks": 1, "inputs": 1})
        for address in ("config", "script", "timeline"):
            trace.write_text('I' + json.dumps({"bundle": {"messages": [{"address": "/input/arena/" + address}]}}) + '\nT\n')
            with self.assertRaises(ValueError):
                trace_counts(trace, expected, (1, 1), forbid_staging=True)
        trace.write_text('T\nT\n')
        with self.assertRaises(ValueError):
            trace_counts(trace, expected, (1, 0))

    def test_native_success_never_labels_unrequested_full_scope_passed(self):
        steps = [{"required": True, "status": "passed"}, {"required": False, "status": "notRequested"}]
        self.assertEqual(completed_scope(steps, {"status": "passed"}), "passed")
        self.assertEqual(completed_scope(steps, {"status": "failed"}), "failed")
        steps[1]["required"] = True
        self.assertEqual(completed_scope(steps, {"status": "passed"}), "failed")

    def test_case_manifest_pins_real_complete_scopes(self):
        cases = json.loads((TOOLS / "arena-verification-cases.json").read_text())
        self.assertEqual(sum(map(len, cases["native"].values())) + len(cases["nativeDoctests"]), 29)
        self.assertEqual([len(cases[key]) for key in ("unityEdit", "unityPlayMsgpack", "unityPlayJson")], [143, 31, 7])
        self.assertTrue(any(name.startswith("ArenaInventoryTests.") for name in cases["unityEdit"]))
        self.assertTrue(any("NetworkInspectionMatchesRenderedReplay" in name for name in cases["unityPlayJson"]))

    def test_guard_restores_owned_original_bytes_including_dirty_or_absent_files(self):
        project = self.directory / "project"
        path = project / "ProjectSettings/ProjectSettings.asset"
        path.parent.mkdir(parents=True)
        path.write_bytes(b"preexisting uncommitted settings\n")
        guard = coordinator.ProjectGuard(project, [])
        path.write_bytes(b"known build settings\n")
        extra = project / "ProjectSettings/ProjectAuditorSettings.asset"
        extra.write_bytes(b"automatic import\n")
        guard.capture_editor()
        self.assertFalse(guard.restore()["unexpectedChanges"])
        self.assertEqual(path.read_bytes(), b"preexisting uncommitted settings\n")
        self.assertFalse(extra.exists())

    def test_guard_never_overwrites_a_new_idle_edit_or_unknown_source(self):
        project = self.directory / "project"
        path = project / "ProjectSettings/ProjectSettings.asset"
        path.parent.mkdir(parents=True)
        path.write_bytes(b"old")
        source = project / "source.cs"
        source.write_bytes(b"original")
        guard = coordinator.ProjectGuard(project, ["source.cs"])
        path.write_bytes(b"build")
        guard.capture_editor()
        path.write_bytes(b"user later")
        with self.assertRaises(RuntimeError):
            guard.verify_idle()
        source.write_bytes(b"unexpected")
        result = guard.restore()
        self.assertEqual(path.read_bytes(), b"user later")
        self.assertIn("source.cs", result["unexpectedChanges"])
        self.assertIn("ProjectSettings/ProjectSettings.asset", result["unexpectedChanges"])

    def test_addressable_guard_allows_only_whitespace_serialization(self):
        self.assertTrue(whitespace_only(b"  field: x\n", b"\n  field: x  \n"))
        for changed in (b"field: x\n", b"  field: y\n", b"  field: x y\n"):
            self.assertFalse(whitespace_only(b"  field: x\n", changed))
        project = self.directory / "project"
        path = project / coordinator.ADDRESSABLE_SETTINGS
        path.parent.mkdir(parents=True)
        path.write_bytes(b"field: old\n")
        guard = coordinator.ProjectGuard(project, [])
        path.write_bytes(b"field: new\n")
        with self.assertRaises(RuntimeError):
            guard.capture_editor()
        self.assertIn(coordinator.ADDRESSABLE_SETTINGS, guard.restore()["unexpectedChanges"])
        self.assertEqual(path.read_bytes(), b"field: new\n")

    def test_failed_prerequisite_retains_report_and_does_not_run_dependencies(self):
        args = argparse.Namespace(evidence=self.directory / "attempt", scope="native", cargo="cargo", profile="dev", editor=None, port=None)
        verification = coordinator.Verification(args)
        with patch.object(verification, "preflight", side_effect=RuntimeError("SDK unavailable")), patch.object(verification, "package") as package:
            self.assertEqual(verification.execute(), 1)
            package.assert_not_called()
        report = json.loads((args.evidence / "verification.json").read_text())
        self.assertEqual(report["status"], "failed")
        self.assertEqual(report["steps"][0]["diagnostic"], "SDK unavailable")
        self.assertTrue(all(step["status"] == "notRequested" for step in report["steps"] if not step["required"]))
        self.assertEqual(report["cleanup"]["status"], "passed")

    def test_broken_evidence_symlink_is_not_resolved_into_another_path(self):
        link = self.directory / "attempt"
        link.symlink_to(self.directory / "absent-target")
        args = argparse.Namespace(evidence=link, scope="native", cargo="cargo", profile="dev", editor=None, port=None)
        with self.assertRaises(ValueError):
            coordinator.Verification(args)
        self.assertFalse((self.directory / "absent-target").exists())

    def test_owned_clone_preserves_symlinks_and_refuses_existing_destination(self):
        source = self.directory / "source"
        source.mkdir()
        (source / "file").write_bytes(b"artifact")
        (source / "link").symlink_to("file")
        destination = self.directory / "destination"
        clone_tree(source, destination)
        self.assertTrue((destination / "link").is_symlink())
        with self.assertRaises(ValueError):
            clone_tree(source, destination)

    def wait_path(self, path):
        deadline = time.monotonic() + 5
        while not path.exists() and time.monotonic() < deadline:
            time.sleep(.02)
        self.assertTrue(path.exists(), "test child did not start")

    def assert_dead(self, pid):
        deadline = time.monotonic() + 3
        while time.monotonic() < deadline:
            try:
                os.kill(pid, 0)
            except ProcessLookupError:
                return
            proc = Path(f"/proc/{pid}/stat")
            if proc.exists() and proc.read_text().split()[2] == "Z":
                return
            time.sleep(.02)
        self.fail(f"owned test child {pid} survived cleanup")

    def test_owned_group_reaps_descendant_even_after_leader_exits(self):
        pidfile = self.directory / "pid"
        child = "import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(60)"
        parent = ("import subprocess,sys,pathlib; p=subprocess.Popen([sys.executable,'-c'," + repr(child) + "]);"
                  "pathlib.Path(" + repr(str(pidfile)) + ").write_text(str(p.pid))")
        with OwnedProcess([sys.executable, "-c", parent], log=self.directory / "group.log", shutdown_grace=.1) as owned:
            self.assertEqual(owned.wait(5), 0)
            self.wait_path(pidfile)
        self.assert_dead(int(pidfile.read_text()))

    def test_wrapper_sigterm_forwards_cleanup_to_its_separate_child_group(self):
        pidfile = self.directory / "pid"
        child = "import os,pathlib,time;pathlib.Path(" + repr(str(pidfile)) + ").write_text(str(os.getpid()));time.sleep(60)"
        wrapper = ("import sys;sys.path.insert(0," + repr(str(TOOLS)) + ");from arena_macos import run;"
                   "run([sys.executable,'-c'," + repr(child) + "],log=" + repr(str(self.directory / 'nested.log')) + ")")
        with OwnedProcess([sys.executable, "-c", wrapper], log=self.directory / "wrapper.log", shutdown_grace=3) as owned:
            self.wait_path(pidfile)
            owned.stop()
        self.assert_dead(int(pidfile.read_text()))


if __name__ == "__main__":
    unittest.main()
