"""Portable file-packaging checks; synthetic Player trees are not runnable Players.

The packager treats TMD/Rhai/TSQ1 as bounded opaque source bytes. Runtime parsing,
Python-to-Rust interoperability and graphical Addressables loading have separate
checks. These fixtures exercise only this module's file/manifest boundary.
"""

import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import tempfile
import unittest
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import arena_content  # noqa: E402


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")


def mapping():
    return {"schemaVersion": 1, "assets": [
        {"role": role, "key": f"synthetic/arena/{role}", "type": kind}
        for role, kind in arena_content.ROLES
    ]}


class PackageFixture(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="arena-content-test-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.source = self.root / "source"
        self.destination = self.root / "package"
        self.payloads = {
            "unity-assets.json": encoded(mapping()),
            "arena.tmd": b"# Synthetic packager fixture; no game-rule parsing here.\n",
            "boss.rhai": b"// Synthetic packager fixture, not an executable boss.\n",
            "timelines/boss-telegraph.tsq": b"synthetic-boss-timeline-file-verifier-only",
            "timelines/floor-transition.tsq": b"synthetic-floor-timeline-file-verifier-only",
        }
        for name, payload in self.payloads.items():
            path = self.source / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(payload)
        self.original = arena_content.stage_package(self.source, self.destination)

    def write_manifest(self, manifest):
        (self.destination / "package.json").write_bytes(encoded(manifest))

    def manifest(self):
        return json.loads((self.destination / "package.json").read_bytes())

    def assert_rejected(self, action, message=None):
        with self.assertRaises((ValueError, OSError)) as error:
            action()
        if message is not None:
            self.assertIn(message, str(error.exception))


class PackageManifestTests(PackageFixture):
    def test_five_file_manifest_bytes_hash_and_order_are_deterministic(self):
        expected = {"schemaVersion": 1, "files": [
            {"path": name, "bytes": len(self.payloads[name]),
             "sha256": hashlib.sha256(self.payloads[name]).hexdigest()}
            for name, _maximum in arena_content.FILES
        ]}
        manifest_bytes = encoded(expected)
        self.assertEqual((self.destination / "package.json").read_bytes(), manifest_bytes)
        self.assertEqual(self.original["files"], expected["files"])
        self.assertEqual(self.original["hash"], hashlib.sha256(manifest_bytes).hexdigest())
        self.assertEqual(self.original["manifestBytes"], len(manifest_bytes))
        self.assertEqual(self.original["assets"], mapping()["assets"])
        for name, payload in self.payloads.items():
            self.assertEqual((self.destination / name).read_bytes(), payload)
        again = arena_content.stage_package(self.source, self.root / "second")
        self.assertEqual(again["hash"], self.original["hash"])
        self.assertEqual((self.root / "second/package.json").read_bytes(), manifest_bytes)

    def test_identical_restage_preserves_existing_files_and_unrelated_content(self):
        tracked = [self.destination / name for name in [*self.payloads, "package.json"]]
        for path in tracked:
            os.utime(path, ns=(1_600_000_000_123456789, 1_600_000_000_123456789))
        mtimes = [path.stat().st_mtime_ns for path in tracked]
        unrelated = self.destination / "operator-note.txt"
        unrelated.write_bytes(b"keep this author note")
        again = arena_content.stage_package(self.source, self.destination)
        self.assertEqual(again["hash"], self.original["hash"])
        self.assertEqual([path.stat().st_mtime_ns for path in tracked], mtimes)
        self.assertEqual(unrelated.read_bytes(), b"keep this author note")

    def test_changed_source_gets_new_digest_and_package_identity(self):
        changed = self.payloads["boss.rhai"] + b"// changed\n"
        (self.source / "boss.rhai").write_bytes(changed)
        current = arena_content.stage_package(self.source, self.destination)
        self.assertNotEqual(current["hash"], self.original["hash"])
        self.assertEqual(current["files"][2]["sha256"], arena_content.digest(changed))
        self.assertEqual(current["files"][:2], self.original["files"][:2])
        self.assertEqual(current["files"][3:], self.original["files"][3:])

    def test_missing_or_tampered_payload_is_rejected(self):
        for name, payload in self.payloads.items():
            with self.subTest(name=name, kind="missing"):
                path = self.destination / name
                path.unlink()
                self.assert_rejected(lambda: arena_content.inspect_package(self.destination))
                path.write_bytes(payload)
            with self.subTest(name=name, kind="changed"):
                path.write_bytes(payload + b"changed")
                self.assert_rejected(lambda: arena_content.inspect_package(self.destination), "does not match")
                path.write_bytes(payload)

    def test_bad_source_is_validated_before_existing_generation_is_replaced(self):
        manifest = (self.destination / "package.json").read_bytes()
        (self.source / "boss.rhai").write_bytes(b"next generation")
        (self.source / "timelines/floor-transition.tsq").unlink()
        self.assert_rejected(lambda: arena_content.stage_package(self.source, self.destination))
        self.assertEqual((self.destination / "package.json").read_bytes(), manifest)
        self.assertEqual(arena_content.inspect_package(self.destination)["hash"], self.original["hash"])

    def test_manifest_schema_and_exact_fields(self):
        original = self.manifest()
        variants = [None, [], {}, {**original, "unexpected": True}]
        variants += [{**original, "schemaVersion": value} for value in [True, False, 1.0, "1", 0, 2, None]]
        for value in variants:
            with self.subTest(value=value):
                self.write_manifest(value)
                self.assert_rejected(lambda: arena_content.inspect_package(self.destination))

    def test_manifest_requires_exact_ordered_paths_and_file_count(self):
        original = self.manifest()
        rows = original["files"]
        variants = [rows[:-1], rows + [rows[0]], list(reversed(rows)), [rows[0]] * 5, None, {}]
        for value in variants:
            with self.subTest(files=value):
                self.write_manifest({**original, "files": value})
                self.assert_rejected(lambda: arena_content.inspect_package(self.destination))
        for name in ["../outside", "/tmp/outside", "timelines\\boss-telegraph.tsq", "./unity-assets.json"]:
            with self.subTest(path=name):
                candidate = copy.deepcopy(original)
                candidate["files"][0]["path"] = name
                self.write_manifest(candidate)
                self.assert_rejected(lambda: arena_content.inspect_package(self.destination), "Expected packaged path")

    def test_manifest_file_byte_count_and_digest_types_are_strict(self):
        original = self.manifest()
        variants = [{"bytes": value} for value in [True, 1.0, "1", 0, -1, 8193, None]]
        variants += [{"sha256": value} for value in [None, 1, "", "g" * 64, "A" * 64, "a" * 63, "a" * 65]]
        variants += [{"extra": 0}]
        for changes in variants:
            with self.subTest(changes=changes):
                candidate = copy.deepcopy(original)
                candidate["files"][0].update(changes)
                self.write_manifest(candidate)
                self.assert_rejected(lambda: arena_content.inspect_package(self.destination))
        candidate = copy.deepcopy(original)
        del candidate["files"][0]["sha256"]
        self.write_manifest(candidate)
        self.assert_rejected(lambda: arena_content.inspect_package(self.destination))

    def test_manifest_digest_cannot_claim_other_payload_of_equal_length(self):
        original = self.manifest()
        original["files"][2]["sha256"] = "0" * 64
        self.write_manifest(original)
        self.assert_rejected(lambda: arena_content.inspect_package(self.destination), "does not match")

    def test_source_and_manifest_byte_boundaries(self):
        for name, maximum in arena_content.FILES:
            with self.subTest(name=name):
                path = self.source / name
                payload = self.payloads[name]
                for invalid in [b"", b"x" * (maximum + 1)]:
                    path.write_bytes(invalid)
                    self.assert_rejected(lambda: arena_content.stage_package(self.source, self.destination), "must contain")
                path.write_bytes(payload + b" " * (maximum - len(payload)))
        maximum_package = arena_content.stage_package(self.source, self.destination)
        self.assertEqual([entry["bytes"] for entry in maximum_package["files"]], [limit for _name, limit in arena_content.FILES])
        manifest = (self.destination / "package.json").read_bytes()
        path = self.destination / "package.json"
        path.write_bytes(manifest + b" " * (arena_content.MANIFEST_LIMIT - len(manifest)))
        self.assertEqual(arena_content.inspect_package(self.destination)["manifestBytes"], arena_content.MANIFEST_LIMIT)
        for invalid in [b"", path.read_bytes() + b" "]:
            path.write_bytes(invalid)
            self.assert_rejected(lambda: arena_content.inspect_package(self.destination), "must contain")

    def test_duplicate_json_keys_and_non_json_float_constants_are_rejected(self):
        for document in [b'{"schemaVersion":1,"schemaVersion":1,"files":[]}',
                         b'{"nested":{"a":1,"a":2}}', b'{"x":NaN}', b'{"x":Infinity}',
                         b'{"x":-Infinity}', b'{"x":1} trailing', b'\xff', b'\xef\xbb\xbf{}']:
            with self.subTest(document=document):
                self.assert_rejected(lambda: arena_content.read_json(document))


class VisualMappingTests(PackageFixture):
    def test_mapping_roles_order_type_fields_and_unique_keys(self):
        variants = []
        for value in [True, 1.0, "1", 2, None]:
            candidate = mapping(); candidate["schemaVersion"] = value; variants.append(candidate)
        candidate = mapping(); candidate["extra"] = True; variants.append(candidate)
        candidate = mapping(); candidate["assets"].reverse(); variants.append(candidate)
        candidate = mapping(); candidate["assets"].pop(); variants.append(candidate)
        candidate = mapping(); candidate["assets"][0]["type"] = "GameObject"; variants.append(candidate)
        candidate = mapping(); candidate["assets"][0]["role"] = "unknown"; variants.append(candidate)
        candidate = mapping(); candidate["assets"][0]["extra"] = True; variants.append(candidate)
        candidate = mapping(); candidate["assets"][1]["key"] = candidate["assets"][0]["key"]; variants.append(candidate)
        for index, candidate in enumerate(variants):
            with self.subTest(variant=index):
                self.assert_rejected(lambda: arena_content.visual_mapping(encoded(candidate)))

    def test_mapping_keys_use_utf8_byte_limit_and_reject_nul(self):
        for value in ["", "bad\0key", 1, None, "x" * 129, "é" * 65]:
            with self.subTest(key=value):
                candidate = mapping(); candidate["assets"][0]["key"] = value
                self.assert_rejected(lambda: arena_content.visual_mapping(encoded(candidate)))
        for value in ["x" * 128, "é" * 64]:
            candidate = mapping(); candidate["assets"][0]["key"] = value
            self.assertEqual(arena_content.visual_mapping(encoded(candidate))[0]["key"], value)

    def test_malformed_mapping_cannot_be_hidden_behind_matching_manifest_digest(self):
        path = self.destination / "unity-assets.json"
        path.write_bytes(b'{"schemaVersion":1,"schemaVersion":1,"assets":[]}')
        manifest = self.manifest()
        manifest["files"][0].update(bytes=path.stat().st_size, sha256=arena_content.digest(path.read_bytes()))
        self.write_manifest(manifest)
        self.assert_rejected(lambda: arena_content.inspect_package(self.destination), "Duplicate JSON field")


class PackagePathTests(PackageFixture):
    def test_source_destination_and_inspection_root_symlinks_are_rejected(self):
        for name, target, operation in [
                ("source-link", self.source, lambda link: arena_content.stage_package(link, self.destination)),
                ("destination-link", self.destination, lambda link: arena_content.stage_package(self.source, link)),
                ("inspection-link", self.destination, arena_content.inspect_package)]:
            with self.subTest(name=name):
                link = self.root / name
                link.symlink_to(target, target_is_directory=True)
                self.assert_rejected(lambda: operation(link), "symlink")

    def test_same_source_and_destination_is_refused(self):
        self.assert_rejected(lambda: arena_content.stage_package(self.source, self.source), "must differ")

    def test_nested_source_and_destination_directory_symlinks_are_rejected(self):
        outside = self.root / "outside-timelines"
        shutil.copytree(self.source / "timelines", outside)
        marker = outside / "marker"; marker.write_bytes(b"untouched")
        for base, operation in [(self.source, lambda: arena_content.stage_package(self.source, self.destination)),
                                (self.destination, lambda: arena_content.stage_package(self.source, self.destination))]:
            with self.subTest(base=base.name):
                directory = base / "timelines"
                shutil.rmtree(directory)
                directory.symlink_to(outside, target_is_directory=True)
                self.assert_rejected(operation, "symlink")
                self.assertEqual(marker.read_bytes(), b"untouched")
                directory.unlink(); shutil.copytree(outside, directory)

    def test_source_and_packaged_file_symlinks_do_not_follow_external_bytes(self):
        outside = self.root / "outside.rhai"; outside.write_bytes(self.payloads["boss.rhai"])
        for base, operation in [(self.source, lambda: arena_content.stage_package(self.source, self.destination)),
                                (self.destination, lambda: arena_content.inspect_package(self.destination))]:
            with self.subTest(base=base.name):
                path = base / "boss.rhai"; path.unlink(); path.symlink_to(outside)
                self.assert_rejected(operation, "symlink")
                self.assertEqual(outside.read_bytes(), self.payloads["boss.rhai"])
                path.unlink(); path.write_bytes(self.payloads["boss.rhai"])

    def test_destination_file_symlink_cannot_overwrite_its_target(self):
        outside = self.root / "outside.rhai"; outside.write_bytes(b"do not replace")
        path = self.destination / "boss.rhai"; path.unlink(); path.symlink_to(outside)
        self.assert_rejected(lambda: arena_content.stage_package(self.source, self.destination), "symlink")
        self.assertEqual(outside.read_bytes(), b"do not replace")

    def test_preexisting_predictable_temporary_symlink_is_not_opened(self):
        outside = self.root / "outside"; outside.write_bytes(b"untouched")
        link = self.destination / "boss.rhai.tmp"; link.symlink_to(outside)
        (self.destination / "boss.rhai").write_bytes(b"stale")
        current = arena_content.stage_package(self.source, self.destination)
        self.assertEqual(current["hash"], self.original["hash"])
        self.assertEqual(outside.read_bytes(), b"untouched")
        self.assertTrue(link.is_symlink())
        self.assertEqual(list(self.destination.glob("boss.rhai.*")), [link])

    def test_failed_temporary_replace_cleans_up_and_keeps_previous_manifest(self):
        previous = (self.destination / "package.json").read_bytes()
        (self.source / "boss.rhai").write_bytes(b"changed")
        with mock.patch.object(Path, "replace", side_effect=OSError("synthetic replace failure")):
            self.assert_rejected(lambda: arena_content.stage_package(self.source, self.destination), "synthetic replace failure")
        self.assertEqual((self.destination / "package.json").read_bytes(), previous)
        self.assertEqual(list(self.destination.glob("boss.rhai.*")), [])
        self.assertEqual(arena_content.inspect_package(self.destination)["hash"], self.original["hash"])

    def test_directory_and_fifo_are_rejected_before_attempting_to_read(self):
        directory = self.root / "not-file"; directory.mkdir()
        self.assert_rejected(lambda: arena_content.bounded(directory, 128), "regular files")
        if hasattr(os, "mkfifo"):
            fifo = self.root / "fifo"; os.mkfifo(fifo)
            self.assert_rejected(lambda: arena_content.bounded(fifo, 128), "regular files")


class SyntheticPlayerFileVerifierTests(PackageFixture):
    """Synthetic hashes and bundle/catalog bytes test boundaries, never Unity use."""

    def setUp(self):
        super().setUp()
        self.player = self.root / "SyntheticFileVerifier.app"
        self.streaming = self.player / "Contents/Resources/Data/StreamingAssets"
        shutil.copytree(self.destination, self.streaming / "KituArena")
        self.aa = self.streaming / "aa"
        self.artifacts = {"catalog.bin": b"synthetic binary catalog (not Unity-readable)",
                          "settings.json": b'{"syntheticFileVerifierFixture":true}',
                          "StandaloneOSX/arena.bundle": b"synthetic bundle (not Unity-readable)"}
        for name, data in self.artifacts.items():
            path = self.aa / name; path.parent.mkdir(parents=True, exist_ok=True); path.write_bytes(data)
        self.report = {
            "packageVersion": "2.11.2", "remoteCatalog": False, "builder": "BuildScriptPackedMode",
            "package": {"identity": self.original["hash"]},
            "artifacts": [{"relativePath": name, "bytes": len(data), "sha256": arena_content.digest(data)} for name, data in self.artifacts.items()],
            "catalog": {"sha256": arena_content.digest(self.artifacts["catalog.bin"])},
            "settings": {"sha256": arena_content.digest(self.artifacts["settings.json"])},
            "locations": [{"key": asset["key"], "type": "UnityEngine." + asset["type"],
                           "provider": "UnityEngine.ResourceManagement.ResourceProviders.BundledAssetProvider",
                           "internalId": "Assets/Synthetic/" + asset["role"]}
                          for asset in mapping()["assets"]] + [{
                              "key": "synthetic-bundle", "type": "UnityEngine.AssetBundle",
                              "provider": "UnityEngine.ResourceManagement.ResourceProviders.AssetBundleProvider",
                              "internalId": "{UnityEngine.AddressableAssets.Addressables.RuntimePath}/StandaloneOSX/arena.bundle"}],
        }

    def verify(self, report=None):
        return arena_content.verify_player_content(self.player, self.report if report is None else report, self.original)

    def test_synthetic_file_verifier_fixture_is_valid_and_relocatable(self):
        result = self.verify()
        self.assertTrue(result["localOnly"])
        self.assertIn("separate graphical Player probes", result["runtimeUse"])
        self.assertEqual(result["catalogSha256"], self.report["catalog"]["sha256"])
        self.assertEqual(result["artifacts"], self.report["artifacts"])
        relocated = self.root / "relocated" / "Synthetic.app"
        shutil.copytree(self.player, relocated)
        moved = arena_content.verify_player_content(relocated, self.report, self.original)
        self.assertEqual(moved["package"]["hash"], result["package"]["hash"])
        self.assertNotEqual(moved["package"]["directory"], result["package"]["directory"])

    def test_zero_or_multiple_streaming_assets_directories_are_rejected(self):
        other = self.player / "Other/StreamingAssets"; other.mkdir(parents=True)
        self.assert_rejected(self.verify, "exactly one StreamingAssets")
        shutil.rmtree(other)
        shutil.rmtree(self.streaming)
        self.assert_rejected(self.verify, "exactly one StreamingAssets")

    def test_source_package_identity_must_match_installed_and_reviewed_build(self):
        report = copy.deepcopy(self.report); report["package"]["identity"] = "a" * 64
        self.assert_rejected(lambda: self.verify(report), "source package differs")
        source = {**self.original, "hash": "b" * 64}
        self.assert_rejected(lambda: arena_content.verify_player_content(self.player, self.report, source), "source package differs")
        (self.streaming / "KituArena/boss.rhai").write_bytes(b"tampered")
        self.assert_rejected(self.verify, "does not match")

    def test_addressables_version_builder_and_remote_policy_are_enforced(self):
        for changes in [{"packageVersion": "2.11.1"}, {"builder": "BuildScriptFastMode"}, {"remoteCatalog": True}, {"remoteCatalog": 0}]:
            with self.subTest(changes=changes):
                report = copy.deepcopy(self.report); report.update(changes)
                self.assert_rejected(lambda: self.verify(report), "required local packed")

    def test_artifact_relative_path_boundary_and_duplicate_reports(self):
        for path in ["", "/tmp/catalog.bin", "../catalog.bin", "StandaloneOSX/../../arena.bundle", "StandaloneOSX\\arena.bundle", None]:
            with self.subTest(path=path):
                report = copy.deepcopy(self.report); report["artifacts"][0]["relativePath"] = path
                self.assert_rejected(lambda: self.verify(report))
        report = copy.deepcopy(self.report); report["artifacts"].append(report["artifacts"][0])
        self.assert_rejected(lambda: self.verify(report), "Missing or duplicate")

    def test_missing_tampered_or_unreported_bundle_is_rejected(self):
        bundle = self.aa / "StandaloneOSX/arena.bundle"
        original = bundle.read_bytes()
        bundle.unlink(); self.assert_rejected(self.verify, "Missing or duplicate")
        bundle.write_bytes(original + b"tampered"); self.assert_rejected(self.verify, "differs from the content build")
        bundle.write_bytes(original)
        (bundle.parent / "extra.bundle").write_bytes(b"unreported")
        self.assert_rejected(self.verify, "unexpected Addressables bundles")

    def test_reported_artifact_size_and_digest_are_verified(self):
        for changes in [{"bytes": 0}, {"sha256": "a" * 64}]:
            with self.subTest(changes=changes):
                report = copy.deepcopy(self.report); report["artifacts"][1].update(changes)
                self.assert_rejected(lambda: self.verify(report), "differs from the content build")

    def test_catalog_uniqueness_and_separate_settings_identity_are_verified(self):
        extra = self.aa / "catalog-extra.bin"; extra.write_bytes(b"extra")
        self.assert_rejected(self.verify, "binary catalog")
        extra.unlink()
        for field in ["catalog", "settings"]:
            with self.subTest(field=field):
                report = copy.deepcopy(self.report); report[field]["sha256"] = "c" * 64
                self.assert_rejected(lambda: self.verify(report))

    def test_catalog_locations_must_use_local_approved_providers_and_existing_bundles(self):
        prefix = "{UnityEngine.AddressableAssets.Addressables.RuntimePath}/"
        for identifier in ["https://example.invalid/bundle", "/outside/bundle", "unpacked.bundle", prefix + "../outside.bundle", prefix + "missing.bundle"]:
            with self.subTest(identifier=identifier):
                report = copy.deepcopy(self.report); report["locations"][-1]["internalId"] = identifier
                self.assert_rejected(lambda: self.verify(report))
        report = copy.deepcopy(self.report); report["locations"][0]["provider"] = "Custom.FileProvider"
        self.assert_rejected(lambda: self.verify(report), "unexpected provider")
        report = copy.deepcopy(self.report); report["locations"][0]["internalId"] = "file:///outside/asset"
        self.assert_rejected(lambda: self.verify(report), "external load location")

    def test_catalog_keys_and_types_must_match_every_mapped_role_exactly_once(self):
        for change in [{"key": "missing"}, {"type": "UnityEngine.Texture2D"}]:
            with self.subTest(change=change):
                report = copy.deepcopy(self.report); report["locations"][0].update(change)
                self.assert_rejected(lambda: self.verify(report), "requested key/type")
        report = copy.deepcopy(self.report); report["locations"].append(copy.deepcopy(report["locations"][0]))
        self.assert_rejected(lambda: self.verify(report), "requested key/type")

    def test_streaming_assets_directory_cannot_resolve_outside_the_player(self):
        external = self.root / "external-streaming-assets"
        shutil.move(self.streaming, external)
        self.streaming.symlink_to(external, target_is_directory=True)
        self.assertFalse(external.is_relative_to(self.player))
        self.assert_rejected(self.verify, "StreamingAssets directory must be local")
        self.assertEqual((external / "aa/catalog.bin").read_bytes(), self.artifacts["catalog.bin"])

    def test_addressables_root_and_nested_artifact_symlinks_are_rejected(self):
        outside = self.root / "outside-aa"; shutil.copytree(self.aa, outside)
        bundle = self.aa / "StandaloneOSX/arena.bundle"; bundle.unlink(); bundle.symlink_to(outside / "StandaloneOSX/arena.bundle")
        self.assert_rejected(self.verify, "symlink")
        bundle.unlink(); bundle.write_bytes(self.artifacts["StandaloneOSX/arena.bundle"])
        shutil.rmtree(self.aa); self.aa.symlink_to(outside, target_is_directory=True)
        self.assert_rejected(self.verify, "Addressables directory must be local")


if __name__ == "__main__":
    unittest.main()
