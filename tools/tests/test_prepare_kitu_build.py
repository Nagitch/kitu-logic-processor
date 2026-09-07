"""Graph preparation tests with real filesystem inputs and synthetic Cargo metadata."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location("prepare_kitu", Path(__file__).parents[1] / "prepare-kitu-build.py")
prepare = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(prepare)


class PreparationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.make_tree(self.root)

    def make_tree(self, root):
        for name in ("app", "kitu/crates/kitu-core", "kitu/crates/kitu-runtime"):
            directory = root / name
            directory.mkdir(parents=True)
            (directory / "src").mkdir()
            (directory / "src/lib.rs").write_text("pub fn example() {}\n")
            (directory / "Cargo.toml").write_text(f'[package]\nname = "{directory.name}"\nversion = "0.1.0"\n')
        (root / "Cargo.toml").write_text('[workspace]\nmembers = ["app"]\n')
        (root / "kitu/Cargo.toml").write_text('[workspace]\nmembers = ["crates/kitu-core", "crates/kitu-runtime"]\n')
        (root / "Cargo.lock").write_text('version = 4\n' + ''.join(
            f'[[package]]\nname = "{name}"\nversion = "0.1.0"\n' for name in ("app", "kitu-core", "kitu-runtime")))

    def metadata(self, root=None):
        root = root or self.root
        names = ("app", "kitu/crates/kitu-core", "kitu/crates/kitu-runtime")
        packages = [{"id": str(root / name), "name": Path(name).name, "version": "0.1.0", "source": None,
                     "manifest_path": str(root / name / "Cargo.toml"), "targets": []} for name in names]
        nodes = [{"id": p["id"], "features": [], "deps": []} for p in packages]
        nodes[0]["deps"] = [{"name": p["name"], "pkg": p["id"], "dep_kinds": [{"kind": None, "target": None}]} for p in packages[1:]]
        return {"workspace_root": str(root), "packages": packages, "resolve": {"nodes": nodes}}

    def prepared(self, metadata=None, root=None):
        return prepare.prepare(metadata or self.metadata(root), (root or self.root) / "app/Cargo.toml", "aarch64-apple-darwin")

    def test_relocation_preserves_graph_and_logical_names(self):
        relocated = self.root / "relocated"
        self.make_tree(relocated)
        original, moved = self.prepared(), self.prepared(root=relocated)
        self.assertEqual(original["graph"], moved["graph"])
        self.assertEqual(original["build_configuration"], moved["build_configuration"])
        self.assertEqual([p["logical"] for p in original["roots"]], [p["logical"] for p in moved["roots"]])
        self.assertNotEqual(original["roots"][0]["path"], moved["roots"][0]["path"])

    def test_fresh_source_bytes_are_not_frozen_in_map(self):
        original = self.prepared()
        (self.root / "kitu/crates/kitu-core/src/lib.rs").write_text("pub fn modified() {}\n")
        self.assertEqual(original, self.prepared())
        self.assertIn("src", original["roots"][1]["inputs"])

    def test_effective_features_and_patch_origin_are_in_graph(self):
        original = self.prepared()
        metadata = self.metadata()
        metadata["resolve"]["nodes"][1]["features"] = ["new-feature"]
        self.assertNotEqual(original["graph"], self.prepared(metadata)["graph"])

    def test_profile_settings_are_identity_inputs(self):
        original = self.prepared()
        with (self.root / "Cargo.toml").open("a") as f:
            f.write('[profile.release]\noverflow-checks = true\n')
        self.assertNotEqual(original["build_configuration"], self.prepared()["build_configuration"])

    def test_rejects_mixed_kitu_origins(self):
        metadata = self.metadata()
        metadata["packages"][1]["source"] = "git+https://example.invalid/kitu#abc"
        with self.assertRaisesRegex(ValueError, "mixed Kitu"):
            self.prepared(metadata)

    def test_rejects_missing_source_manifest(self):
        (self.root / "kitu/crates/kitu-core/Cargo.toml").unlink()
        with self.assertRaises(OSError):
            self.prepared()

    def test_rejects_missing_locked_package(self):
        (self.root / "Cargo.lock").write_text("version = 4\n")
        with self.assertRaisesRegex(ValueError, "missing from Cargo.lock"):
            self.prepared()

    def test_rejects_wrong_application(self):
        metadata = self.metadata()
        metadata["packages"][0]["manifest_path"] = str(self.root / "other/Cargo.toml")
        with self.assertRaisesRegex(ValueError, "exactly the requested"):
            self.prepared(metadata)

    def test_resolves_only_application_dependency_closure(self):
        metadata = self.metadata()
        metadata["packages"].append({"id": "unrelated", "name": "unrelated", "manifest_path": str(self.root / "unrelated/Cargo.toml")})
        self.assertEqual(self.prepared()["graph"], self.prepared(metadata)["graph"])


if __name__ == "__main__":
    unittest.main()
