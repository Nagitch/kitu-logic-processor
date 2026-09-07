#!/usr/bin/env python3
"""Verify real graphical packaged-content startup, including explicit failures.

This probe does not replace the exact gameplay-trace verifier. It independently
checks the bootstrap report and owned cleanup, and retains the actual invocation,
local resource paths, package/native identities and evidence of relocation.
"""

import argparse
import datetime
import json
import os
from pathlib import Path
import plistlib
import time
from urllib.parse import unquote, urlparse

from arena_content import inspect_package
from arena_macos import LIBRARY, ROOT, artifact, clone_tree, require_macos, run, verify_plugin, write_json


def within(path, parent):
    return path == parent or parent in path.parents


def local_bundle_locations(asset_report, player):
    """Read actual runtime locations, rather than guessing copied bundle paths."""
    bundles = set()
    pending = list(asset_report.get("loaded", []))
    while pending:
        value = pending.pop()
        if isinstance(value, list):
            pending.extend(value)
            continue
        if not isinstance(value, dict):
            continue
        provider = value.get("provider", "")
        if "assetdatabase" in provider.lower():
            raise RuntimeError("Player resolved an Editor AssetDatabase provider")
        internal = value.get("resolvedInternalId", value.get("internalId"))
        if isinstance(internal, str):
            if internal.startswith(("http://", "https://")):
                raise RuntimeError(f"Player used a remote resource: {internal}")
            if ".bundle" in internal:
                parsed = urlparse(internal)
                if parsed.scheme == "file":
                    if parsed.netloc not in ("", "localhost"):
                        raise RuntimeError(f"Nonlocal bundle URL: {internal}")
                    path = Path(unquote(parsed.path))
                elif parsed.scheme:
                    raise RuntimeError(f"Unsupported bundle URL: {internal}")
                else:
                    path = Path(internal)
                if not path.is_absolute():
                    raise RuntimeError(f"Runtime report has no resolved absolute bundle location: {internal}")
                path = path.resolve()
                if not within(path, player) or not path.is_file():
                    raise RuntimeError(f"Runtime bundle is missing or outside the selected Player: {path}")
                bundles.add(path)
        pending.extend(child for child in value.values() if isinstance(child, (dict, list)))
    if not bundles:
        raise RuntimeError("Runtime report did not identify any actual local Addressables bundle dependency")
    return [artifact(path) for path in sorted(bundles)]


def verify_result(result, *, expected_failure, player, package, expected_package_hash=None):
    if (result.get("schemaVersion") != 1 or result.get("matched") is not True
            or result.get("expectedFailure") != expected_failure):
        raise RuntimeError(f"Content probe did not confirm the requested outcome: {result}")
    if result.get("tick") != -1 or result.get("referenceGameCount") != 0:
        raise RuntimeError("Bootstrap advanced a Runtime tick or created the procedural reference game")
    for field in ("nativeHandlesAfterDisable", "nativeHandlesAfterCleanup", "assetHandlesAfterCleanup"):
        if result.get(field) != 0:
            raise RuntimeError(f"Content probe leaked ownership: {field}={result.get(field)}")
    if Path(result.get("packageDirectory", "")).resolve() != package:
        raise RuntimeError("Player loaded a different package directory than requested")
    if expected_failure is not None:
        if (result.get("connected") is not False or result.get("prepared") is not False
                or result.get("nativeHandles") != 0 or result.get("nativeHandlePeak") != 0
                or result.get("presentationCount") != 0):
            raise RuntimeError("Expected package/asset failure created a Runtime or fallback view")
        if expected_failure.casefold() not in str(result.get("diagnostic", "")).casefold():
            raise RuntimeError("Player failed for a different cause than requested")
        return {"expectedFailureConfirmed": True}
    if (result.get("connected") is not True or result.get("prepared") is not True
            or result.get("nativeHandles") != 1 or result.get("nativeHandlePeak") != 1
            or result.get("assetHandles") != 4 or result.get("presentationCount") != 1):
        raise RuntimeError("Successful bootstrap did not own one Runtime/view and four asset leases")
    package_info = inspect_package(package)
    identity = package_info["hash"]
    asset_report = result.get("assetReport", {})
    if (asset_report.get("package", {}).get("identity") != identity
            or result.get("nativeHost", {}).get("package", {}).get("hash") != identity):
        raise RuntimeError("Player visual/native initialization does not match the actual package bytes")
    if expected_package_hash is not None and identity != expected_package_hash:
        raise RuntimeError("Player package hash differs from the expected authored package")
    loaded = asset_report.get("loaded", [])
    expected_assets = package_info["assets"]
    if len(loaded) != len(expected_assets):
        raise RuntimeError("Player did not report all four actual loaded assets")
    for actual, expected in zip(loaded, expected_assets):
        if any(actual.get(field) != expected[field] for field in ("role", "key", "type")):
            raise RuntimeError(f"Player loaded a different asset role/key/type: {actual}")
    return {"package": package_info, "runtimeBundles": local_bundle_locations(asset_report, player)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--player", required=True, type=Path)
    parser.add_argument("--evidence", required=True, type=Path)
    parser.add_argument("--expect-failure", help="Required diagnostic substring for a pre-connection failure")
    parser.add_argument("--package", type=Path, help="Explicit isolated package override; default is the shipped package")
    parser.add_argument("--relocate-to", type=Path, help="Copy the app to this absent absolute .app path before launch")
    parser.add_argument("--storage", type=Path, help="Owned authoring storage to retain/reuse; default is evidence/authoring")
    parser.add_argument("--expected-package-hash")
    parser.add_argument("--expected-native-sha256", help="Require identical dylib bytes across packaged-data edits")
    parser.add_argument("--timeout", type=float, default=120)
    args = parser.parse_args()
    if args.expect_failure is not None and not args.expect_failure.strip():
        parser.error("--expect-failure requires a nonempty diagnostic substring")
    if args.expect_failure is not None and args.expected_package_hash is not None:
        parser.error("A failed package cannot assert a successfully loaded package hash")
    if args.timeout <= 0:
        parser.error("--timeout must be positive")
    require_macos()
    original = args.player.resolve()
    evidence = args.evidence.resolve()
    if not original.is_dir() or original.suffix != ".app":
        parser.error("--player must be an existing .app directory")
    player = original
    if args.relocate_to is not None:
        if not args.relocate_to.is_absolute():
            parser.error("--relocate-to must be absolute")
        if args.relocate_to.is_symlink():
            parser.error("--relocate-to must not be a symlink")
        player = args.relocate_to.resolve()
        if player.exists() or player.suffix != ".app" or within(player, original):
            parser.error("--relocate-to must be an absent .app outside the source app")
        if within(player, ROOT):
            parser.error("Relocation proof must place the app outside the checkout")
        player.parent.mkdir(parents=True, exist_ok=True)
        clone_tree(original, player)
    with (player / "Contents/Info.plist").open("rb") as source:
        executable_name = plistlib.load(source)["CFBundleExecutable"]
    if not isinstance(executable_name, str) or Path(executable_name).name != executable_name:
        raise RuntimeError("Invalid Player executable basename")
    executable = player / "Contents/MacOS" / executable_name
    plugins = list(player.rglob(LIBRARY))
    if len(plugins) != 1:
        raise RuntimeError("Player must contain exactly one native Arena dylib")
    plugin = verify_plugin(plugins[0])
    if args.expected_native_sha256 is not None and plugin["sha256"] != args.expected_native_sha256:
        raise RuntimeError("Packaged-data proof changed the native dylib bytes")
    # Derive only the package path here. Do not validate it before an expected
    # failure: the actual Player must observe and diagnose the invalid bytes.
    if args.package is None:
        streaming = list(player.glob("Contents/Resources/Data/StreamingAssets"))
        if len(streaming) != 1:
            raise RuntimeError("Cannot locate the Player StreamingAssets directory")
        package = (streaming[0] / "KituArena").resolve()
    else:
        if not args.package.is_absolute():
            parser.error("--package must be absolute")
        package = args.package.resolve()
    storage = (args.storage or evidence / "authoring").resolve()
    if within(storage, player):
        parser.error("Authoring storage must be outside the immutable Player")
    authored = ("arena.tmd", "boss.rhai", "timelines/boss-telegraph.tsq", "timelines/floor-transition.tsq")
    before = {name: artifact(storage / name) for name in authored if (storage / name).is_file()}
    evidence.mkdir(parents=True, exist_ok=True)
    working = evidence / "launch-cwd"
    working.mkdir(exist_ok=True)
    for name in ("content-result.json", "content-player-verification.json"):
        (evidence / name).unlink(missing_ok=True)
    command = [executable, "-screen-fullscreen", "0", "-screen-width", "1100", "-screen-height", "720",
               "-logFile", evidence / "player.log", "--arena-content-self-test",
               "--arena-content-evidence", evidence, "--arena-storage", storage]
    if args.package is not None:
        command.extend(["--arena-package", package])
    if args.expect_failure is not None:
        command.extend(["--arena-content-expect-failure", args.expect_failure])
    env = os.environ.copy()
    for key in ("KITU_ARENA_WS_URL", "KITU_ARENA_ENCODING"):
        env.pop(key, None)
    started = time.time()
    run(command, log=evidence / "launch.log", env=env, timeout=args.timeout, cwd=working)
    result = json.loads((evidence / "content-result.json").read_text())
    checked = verify_result(result, expected_failure=args.expect_failure, player=player,
                            package=package, expected_package_hash=args.expected_package_hash)
    after = {name: artifact(storage / name) for name in authored if (storage / name).is_file()}
    for name, previous in before.items():
        if name not in after or after[name]["sha256"] != previous["sha256"]:
            raise RuntimeError(f"Player overwrote existing authoring source: {name}")
    if args.expect_failure is None:
        for name in authored:
            if name not in after:
                raise RuntimeError(f"Player did not seed its selected authoring directory: {name}")
            if name not in before and after[name]["sha256"] != artifact(package / name)["sha256"]:
                raise RuntimeError(f"Player seeded compiled defaults instead of the selected package: {name}")
    elif set(after) != set(before):
        raise RuntimeError("Failed package bootstrap created new authoring files")
    report = {"verifiedAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "elapsedSeconds": time.time() - started, "originalPlayer": str(original), "player": str(player),
              "relocated": player != original, "command": [str(value) for value in command],
              "workingDirectory": str(working), "authoringStorage": str(storage),
              "authoringBefore": before, "authoringAfter": after,
              "bundledPlugin": plugin, "executable": artifact(executable),
              "result": result, "independentChecks": checked}
    write_json(evidence / "content-player-verification.json", report)
    print(json.dumps({"matched": True, "expectedFailure": args.expect_failure,
                      "evidence": str(evidence / "content-player-verification.json")}, indent=2))


if __name__ == "__main__":
    main()
