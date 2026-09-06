#!/usr/bin/env python3
"""Build the embedded Arena macOS Player and verify its bundled native library."""

import argparse
import datetime
import json
import os
from pathlib import Path
import plistlib

from arena_macos import (LIBRARY, PLUGIN, PROJECT, ROOT, artifact,
                         require_closed_editor, require_macos, run,
                         verify_plugin, write_json)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--editor", type=Path)
    parser.add_argument("--player", type=Path, default=PROJECT / "Builds/KituEndlessArena.app")
    parser.add_argument("--evidence", type=Path, default=ROOT / ".tmp/stage11/player-build")
    parser.add_argument("--timeout", type=float, default=1800)
    args = parser.parse_args()
    require_macos()
    require_closed_editor()
    version = next(line.split(": ", 1)[1] for line in
                   (PROJECT / "ProjectSettings/ProjectVersion.txt").read_text().splitlines()
                   if line.startswith("m_EditorVersion: "))
    editor = args.editor or Path(f"/Applications/Unity/Hub/Editor/{version}/Unity.app/Contents/MacOS/Unity")
    if not editor.is_file():
        raise RuntimeError(f"Install the project's Unity {version} Editor or pass --editor: {editor}")
    source_plugin = verify_plugin(PLUGIN)
    player = args.player.resolve()
    if player.suffix != ".app":
        raise RuntimeError("--player must name a macOS .app bundle")
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    editor_report = evidence / "unity-build.json"
    editor_report.unlink(missing_ok=True)
    env = os.environ.copy()
    env["KITU_ARENA_PLAYER_PATH"] = str(player)
    env["KITU_ARENA_BUILD_REPORT"] = str(editor_report)
    command = [editor, "-batchmode", "-quit", "-projectPath", PROJECT,
               "-buildTarget", "osxuniversal", "-executeMethod",
               "UnityOnlyArena.Editor.KituArenaSceneBuilder.BuildMac",
               "-logFile", evidence / "unity-editor.log"]
    run(command, log=evidence / "unity-launch.log", env=env, timeout=args.timeout)
    report = json.loads(editor_report.read_text())
    if report.get("result") != "Succeeded" or report.get("unityVersion") != version:
        raise RuntimeError(f"Unity build did not verify the pinned Editor and success: {report}")
    with (player / "Contents/Info.plist").open("rb") as file:
        executable_name = plistlib.load(file)["CFBundleExecutable"]
    if Path(executable_name).name != executable_name:
        raise RuntimeError("Player Info.plist has an invalid executable name")
    executable = player / "Contents/MacOS" / executable_name
    if run(["xcrun", "lipo", "-archs", executable]).split() != ["arm64"]:
        raise RuntimeError("Unity Player is not the requested arm64 build")
    copies = list(player.rglob(LIBRARY))
    if len(copies) != 1:
        raise RuntimeError(f"Expected exactly one embedded {LIBRARY}, found {copies}")
    bundled = verify_plugin(copies[0])
    if bundled["unsignedSha256"] != source_plugin["unsignedSha256"]:
        raise RuntimeError("Bundled library code differs from the staged Unity plugin")
    run(["/usr/bin/codesign", "--verify", "--deep", "--strict", player])
    result = {"createdAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "player": str(player), "executable": artifact(executable),
              "unityBuild": report, "sourcePlugin": source_plugin,
              "bundledPlugin": bundled, "signatureVerified": True,
              "nativeLibraryBundled": True,
              "playVerification": "Run run-arena-player-verification.py separately."}
    write_json(evidence / "player-build.json", result)
    print(json.dumps({"player": str(player), "evidence": str(evidence / "player-build.json")}, indent=2))


if __name__ == "__main__":
    main()
