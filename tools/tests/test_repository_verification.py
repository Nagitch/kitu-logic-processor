"""Test the framework-only verifier's report and scope contracts."""

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
SPEC = importlib.util.spec_from_file_location("framework_repository_verification", TOOLS / "verify-repository.py")
verification = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(verification)


class RepositoryVerificationTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="framework-verification-test-")
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.frontend = self.root / "starter"
        self.frontend.mkdir()
        (self.root / "rust-toolchain.toml").write_text('[toolchain]\nchannel = "1.96.0"\n')
        (self.frontend / "package.json").write_text('{"packageManager":"pnpm@11.9.0"}\n')
        self.evidence = self.root / "evidence"
        self.calls = []
        self.outputs = {
            "rust-version": "rustc 1.96.0 (synthetic)\n",
            "test": "test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\n",
            "node-version": "v24.14.0\n",
            "pnpm-version": "11.9.0\n",
            "python-tests": "Ran 12 tests in 0.01s\n\nOK\n",
            "runtime-smoke": "smoke passed\n",
        }
        self.failure_step = None

    def fake_run(self, argv, **kwargs):
        log = kwargs["log"]
        name = log.stem
        self.calls.append((name, [str(value) for value in argv], kwargs))
        report = json.loads((self.evidence / "verification.json").read_text())
        self.assertEqual(report["status"], "running")
        self.assertEqual(report["steps"][-1]["id"], name)
        self.assertEqual(kwargs["env"]["PYTHONDONTWRITEBYTECODE"], "1")
        log.write_text(self.outputs.get(name, "completed\n"))
        if name == self.failure_step:
            raise RuntimeError("synthetic command failed")

    def invoke(self, scope):
        stdout, stderr = io.StringIO(), io.StringIO()
        source = {"commit": "a" * 40, "dirty": False, "dirtyFiles": [], "locks": []}
        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(verification, "ROOT", self.root))
            stack.enter_context(mock.patch.object(verification, "FRONTEND", self.frontend))
            stack.enter_context(mock.patch.object(verification, "source_identity", return_value=source))
            stack.enter_context(mock.patch.object(verification, "run", side_effect=self.fake_run))
            stack.enter_context(mock.patch.object(verification.platform, "platform", return_value="test-platform"))
            stack.enter_context(mock.patch.object(verification.subprocess, "check_output",
                                                  side_effect=AssertionError("unexpected subprocess")))
            stack.enter_context(mock.patch.object(sys, "argv", ["verify-repository.py", "--scope", scope,
                                                                   "--evidence", str(self.evidence)]))
            stack.enter_context(redirect_stdout(stdout))
            stack.enter_context(redirect_stderr(stderr))
            verification.main()
        return json.loads((self.evidence / "verification.json").read_text())

    def test_scope_set_excludes_application_and_macos_proofs(self):
        self.assertEqual(verification.SCOPES, ("fmt", "test", "clippy", "docs", "tools", "frontend"))
        with mock.patch.object(sys, "argv", ["verify-repository.py", "--scope", "native", "--evidence", str(self.evidence)]):
            with self.assertRaises(SystemExit) as error:
                verification.main()
        self.assertEqual(error.exception.code, 2)

    def test_test_scope_records_running_steps_and_pass_counts(self):
        report = self.invoke("test")
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["rustTests"], {"passed": 4, "suites": 1, "failed": 0, "ignored": 0})
        self.assertEqual([name for name, _argv, _kwargs in self.calls], ["rust-version", "test"])
        self.assertEqual(self.calls[-1][1], ["cargo", "test", "--locked", "--workspace"])
        self.assertEqual(self.calls[-1][2]["cwd"], self.root)

    def test_tools_scope_runs_only_framework_tools_and_generic_smoke(self):
        report = self.invoke("tools")
        self.assertEqual(report["status"], "passed")
        self.assertEqual(report["pythonTests"], 12)
        self.assertEqual([name for name, _argv, _kwargs in self.calls], ["rust-version", "python-tests", "runtime-smoke"])
        self.assertIn("kitu-integration-runner/scenarios/smoke/player-move-basic/scenario.json", self.calls[-1][1])

    def test_frontend_requires_pinned_node_and_pnpm_before_install(self):
        self.outputs["node-version"] = "v22.0.0\n"
        with self.assertRaisesRegex(RuntimeError, "Node 24"):
            self.invoke("frontend")
        self.assertEqual([name for name, _argv, _kwargs in self.calls], ["rust-version", "node-version"])

    def test_docs_records_warning_policy(self):
        report = self.invoke("docs")
        self.assertEqual(report["status"], "passed")
        self.assertEqual(self.calls[-1][2]["env"]["RUSTDOCFLAGS"], "-D warnings")
        self.assertIn("--locked", self.calls[-1][1])

    def test_existing_evidence_is_never_overwritten(self):
        self.evidence.mkdir()
        sentinel = self.evidence / "verification.json"
        sentinel.write_text("previous\n")
        with mock.patch.object(verification, "ROOT", self.root), mock.patch.object(sys, "argv",
                ["verify-repository.py", "--scope", "test", "--evidence", str(self.evidence)]):
            with self.assertRaises(SystemExit) as error:
                verification.main()
        self.assertEqual(error.exception.code, 2)
        self.assertEqual(sentinel.read_text(), "previous\n")


if __name__ == "__main__":
    unittest.main()
