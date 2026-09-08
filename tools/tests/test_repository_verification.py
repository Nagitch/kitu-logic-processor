"""Test verification/report orchestration with mocked Cargo/Node/Git commands.

These tests validate the runner, not Rust or frontend results. Actual toolchain
and application checks remain the commands recorded by verify-repository.py.
"""

from contextlib import ExitStack, redirect_stderr, redirect_stdout
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location("arena_repository_verification", TOOLS / "verify-repository.py")
verification = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verification)


class RepositoryVerificationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="arena-repository-verification-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.frontend = self.root / "frontend"
        self.frontend.mkdir()
        (self.root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.96.0"\n')
        (self.frontend / "package.json").write_text('{"packageManager":"pnpm@11.9.0"}\n')
        self.evidence = self.root / "evidence"
        self.calls = []
        self.observed_reports = []
        self.source = {"commit": "a" * 40, "dirty": False, "dirtyFiles": [], "locks": []}
        self.outputs = {
            "rust-version": "rustc 1.96.0 (synthetic version fixture)\n",
            "test": "test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n"
                    "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n",
            "node-version": "v24.14.0\n",
            "pnpm-version": "11.9.0\n",
            "frontend-test-inspection": "# tests 19\n# pass 19\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n",
            "python-tests": "Ran 34 tests in 0.01s\n\nOK\n",
            "package-interop": "Python package interoperability: " + "b" * 64 + "\n",
        }
        self.failure = None
        self.failure_step = None

    def fake_run(self, argv, **kwargs):
        log = kwargs["log"]
        name = log.stem
        self.calls.append((name, [str(value) for value in argv], kwargs))
        current = json.loads((self.evidence / "verification.json").read_text())
        self.observed_reports.append(current)
        self.assertEqual(current["status"], "running")
        self.assertEqual(current["steps"][-1]["id"], name)
        self.assertEqual(current["steps"][-1]["status"], "running")
        self.assertEqual(kwargs["env"]["PYTHONDONTWRITEBYTECODE"], "1")
        log.write_text(self.outputs.get(name, "synthetic command completed\n"))
        if self.failure_step == name:
            raise self.failure

    def invoke(self, scope, *, identity_error=None, package=None):
        stdout, stderr = io.StringIO(), io.StringIO()
        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(verification, "ROOT", self.root))
            stack.enter_context(mock.patch.object(verification, "FRONTEND", self.frontend))
            stack.enter_context(mock.patch.object(verification, "source_identity", side_effect=identity_error, return_value=self.source))
            stack.enter_context(mock.patch.object(verification, "run", side_effect=self.fake_run))
            stack.enter_context(mock.patch.object(verification.platform, "platform", return_value="test-platform"))
            self.staging = stack.enter_context(mock.patch.object(verification, "stage_package", return_value=package or {"hash": "b" * 64, "files": []}))
            # No subprocess may escape the orchestration mocks, even if the
            # runner adds a new direct Git/toolchain query in a later change.
            stack.enter_context(mock.patch.object(verification.subprocess, "check_output", side_effect=AssertionError("unexpected real subprocess")))
            stack.enter_context(mock.patch.object(sys, "argv", ["verify-repository.py", "--scope", scope, "--evidence", str(self.evidence)]))
            stack.enter_context(redirect_stdout(stdout))
            stack.enter_context(redirect_stderr(stderr))
            verification.main()
        return stdout.getvalue()

    def report(self):
        return json.loads((self.evidence / "verification.json").read_text())

    def assert_final(self, status):
        report = self.report()
        self.assertEqual(report["status"], status)
        self.assertIsNotNone(report["finishedAtUtc"])
        for step in report["steps"]:
            self.assertNotEqual(step["status"], "running")
            self.assertIsNotNone(step["finishedAtUtc"])
            self.assertGreaterEqual(step["elapsedSeconds"], 0)
            self.assertTrue(Path(step["logArtifact"]["path"]).is_file())
        return report

    def reset_attempt(self, name):
        self.evidence = self.root / name
        self.calls.clear()
        self.observed_reports.clear()

    def test_fresh_report_tracks_running_steps_and_final_counted_success(self):
        stdout = self.invoke("test")
        report = self.assert_final("passed")
        self.assertEqual(report["source"], self.source)
        self.assertEqual(report["requestedScope"], "test")
        self.assertEqual(report["rustTests"], {"passed": 7, "suites": 2, "failed": 0, "ignored": 0})
        self.assertEqual([step["id"] for step in report["steps"]], ["rust-version", "prepare-test", "test"])
        self.assertEqual(self.calls[2][1], ["cargo", "test", "--locked", "--workspace"])
        self.assertEqual(self.calls[2][2]["cwd"], self.root)
        self.assertEqual(self.calls[2][2]["timeout"], 3600)
        self.assertEqual(json.loads(stdout), {"status": "passed", "report": str(self.evidence / "verification.json")})
        for step in report["steps"]:
            self.assertEqual(step["exitCode"], 0)
            self.assertEqual(step["logArtifact"], verification.artifact(Path(step["log"])))

    def test_failed_identity_preparation_prevents_compilation(self):
        self.failure_step = "prepare-test"
        self.failure = RuntimeError("mixed Kitu sources")
        with self.assertRaisesRegex(RuntimeError, "mixed Kitu"):
            self.invoke("test")
        self.assert_final("failed")
        self.assertNotIn("test", [call[0] for call in self.calls])

    def test_existing_evidence_directory_file_and_dangling_symlink_are_never_overwritten(self):
        for kind in ["directory", "file", "symlink"]:
            with self.subTest(kind=kind):
                self.reset_attempt(kind)
                if kind == "directory":
                    self.evidence.mkdir()
                    sentinel = self.evidence / "verification.json"
                else:
                    sentinel = self.evidence
                if kind == "symlink":
                    self.evidence.symlink_to(self.root / "nonexistent")
                else:
                    sentinel.write_bytes(b"previous result, do not overwrite")
                with self.assertRaises(SystemExit) as error:
                    self.invoke("reference")
                self.assertEqual(error.exception.code, 2)
                self.assertEqual(self.calls, [])
                if kind == "symlink":
                    self.assertTrue(self.evidence.is_symlink())
                else:
                    self.assertEqual(sentinel.read_bytes(), b"previous result, do not overwrite")

    def test_source_identity_failure_leaves_a_fresh_failed_report(self):
        with self.assertRaisesRegex(RuntimeError, "identity unavailable"):
            self.invoke("fmt", identity_error=RuntimeError("identity unavailable"))
        report = self.assert_final("failed")
        self.assertEqual(report["source"], None)
        self.assertEqual(report["steps"], [])
        self.assertEqual(report["diagnostic"], "identity unavailable")
        self.assertEqual(self.calls, [])

    def test_failed_command_and_partial_log_are_retained_without_later_steps(self):
        self.failure_step = "test"
        self.failure = RuntimeError("synthetic command exited 101")
        self.outputs["test"] = "partial compiler diagnostic\n"
        with self.assertRaisesRegex(RuntimeError, "exited 101"):
            self.invoke("all")
        report = self.assert_final("failed")
        self.assertEqual([step["id"] for step in report["steps"]], ["rust-version", "reference", "fmt", "prepare-test", "test"])
        self.assertEqual(report["steps"][-1]["status"], "failed")
        self.assertEqual(report["steps"][-1]["diagnostic"], "synthetic command exited 101")
        self.assertEqual(Path(report["steps"][-1]["log"]).read_text(), "partial compiler diagnostic\n")
        self.assertNotIn("rustTests", report)

    def test_keyboard_interrupt_seals_failed_step_and_report_before_propagating(self):
        self.failure_step = "reference"
        self.failure = KeyboardInterrupt("synthetic cancellation")
        with self.assertRaises(KeyboardInterrupt):
            self.invoke("reference")
        report = self.assert_final("failed")
        self.assertEqual(report["diagnostic"], "synthetic cancellation")
        self.assertEqual(report["steps"][0]["status"], "failed")

    def test_toolchain_mismatch_cannot_run_cargo_after_version_check(self):
        self.outputs["rust-version"] = "rustc 1.95.0 (wrong fixture)\n"
        with self.assertRaisesRegex(RuntimeError, "pinned Rust 1.96.0"):
            self.invoke("fmt")
        self.assert_final("failed")
        self.assertEqual([name for name, _argv, _kwargs in self.calls], ["rust-version"])

    def test_successful_process_exit_does_not_hide_missing_or_ignored_rust_tests(self):
        for index, output in enumerate(["no test results\n", "test result: ok. 0 passed; 0 failed; 0 ignored;\n", "test result: ok. 5 passed; 0 failed; 1 ignored;\n"]):
            with self.subTest(output=output):
                self.reset_attempt(f"rust-count-{index}")
                self.outputs["test"] = output
                with self.assertRaisesRegex(RuntimeError, "execute and pass without ignored"):
                    self.invoke("test")
                report = self.assert_final("failed")
                self.assertEqual(report["steps"][-1]["status"], "passed")
                self.assertNotIn("rustTests", report)

    def test_reference_scope_does_not_require_rust_or_node(self):
        self.invoke("reference")
        self.assert_final("passed")
        self.assertEqual([name for name, _argv, _kwargs in self.calls], ["reference"])
        self.assertEqual(self.calls[0][1], [sys.executable, str(self.root / "tools/verify-arena-reference.py")])

    def test_frontend_uses_pinned_versions_frozen_install_and_real_build_script(self):
        self.invoke("frontend")
        report = self.assert_final("passed")
        self.assertEqual([name for name, _argv, _kwargs in self.calls], ["rust-version", "node-version", "pnpm-version", "shared-admin-install", "shared-admin-build", "shared-admin-check", "shared-admin-test", "starter-install", "starter-check", "starter-build", "frontend-install", "frontend-check", "frontend-lint", "frontend-test-inspection", "frontend-build"])
        self.assertEqual(self.calls[10][1], ["pnpm", "install", "--frozen-lockfile"])
        self.assertEqual(self.calls[10][2]["env"]["CI"], "true")
        self.assertTrue(all(kwargs["cwd"] == self.frontend for name, _argv, kwargs in self.calls if name.startswith("frontend-") or name in ("node-version", "pnpm-version")))
        self.assertEqual(self.calls[-1][1], ["pnpm", "run", "build"])
        self.assertTrue(report["frontendIncludesWasmPrebuild"])
        self.assertEqual(report["frontendTests"]["tests"], 19)

    def test_wrong_node_or_pnpm_version_refuses_install_and_build(self):
        for name, output, reason in [("node-version", "v22.10.0\n", "Node 24"), ("pnpm-version", "11.8.0\n", "pnpm 11.9.0")]:
            with self.subTest(name=name):
                self.reset_attempt(name)
                original = self.outputs[name]
                self.outputs[name] = output
                with self.assertRaisesRegex(RuntimeError, reason):
                    self.invoke("frontend")
                self.assert_final("failed")
                self.assertNotIn("frontend-install", [call[0] for call in self.calls])
                self.outputs[name] = original

    def test_frontend_count_guard_refuses_skips_failures_missing_and_mismatched_totals(self):
        valid = self.outputs["frontend-test-inspection"]
        variants = ["", valid.replace("# pass 19", "# pass 18")]
        variants += [valid.replace(f"# {name} 0", f"# {name} 1") for name in ["fail", "cancelled", "skipped", "todo"]]
        for index, output in enumerate(variants):
            with self.subTest(output=output):
                self.reset_attempt(f"frontend-count-{index}")
                self.outputs["frontend-test-inspection"] = output
                with self.assertRaisesRegex(RuntimeError, "without skips or failures"):
                    self.invoke("frontend")
                self.assert_final("failed")
                self.assertNotIn("frontend-build", [call[0] for call in self.calls])

    def test_data_scope_requires_nonempty_python_suite_and_exact_interop_marker(self):
        for name, output, reason in [("python-tests", "Ran 0 tests in 0.0s\nOK\n", "did not execute"), ("package-interop", "test passed but did not consume package\n", "did not consume")]:
            with self.subTest(name=name):
                self.reset_attempt(name)
                original = self.outputs[name]
                self.outputs[name] = output
                with self.assertRaisesRegex(RuntimeError, reason):
                    self.invoke("data")
                report = self.assert_final("failed")
                self.assertNotIn("package", report)
                self.outputs[name] = original
        self.reset_attempt("data-passed")
        self.invoke("data")
        report = self.assert_final("passed")
        self.assertEqual(report["pythonTests"], 34)
        self.assertEqual(report["package"]["hash"], "b" * 64)
        self.staging.assert_called_once_with(self.root / "apps/demo-game/content", self.evidence / "package")
        call = next(call for call in self.calls if call[0] == "package-interop")
        self.assertEqual(call[2]["env"]["KITU_PACKAGE_INTEROP_DIR"], str(self.evidence / "package"))
        self.assertIn("--locked", call[1])

    def test_docs_command_records_warning_policy(self):
        self.invoke("docs")
        report = self.assert_final("passed")
        self.assertEqual(report["steps"][-1]["environment"], {"RUSTDOCFLAGS": "-D warnings"})
        self.assertIn("--locked", self.calls[-1][1])
        self.assertEqual(self.calls[-1][2]["env"]["RUSTDOCFLAGS"], "-D warnings")


if __name__ == "__main__":
    unittest.main()
