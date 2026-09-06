#!/usr/bin/env python3
"""Build, relocate, sign and install the application-owned macOS Unity plugin."""

import argparse
import datetime
import json
import os
from pathlib import Path
import shutil
import tempfile

from arena_macos import (INSTALL_NAME, LIBRARY, PLUGIN, ROOT, artifact,
                         require_closed_editor, require_macos, run,
                         verify_plugin, write_json)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    parser.add_argument("--profile", choices=["dev", "release"], default="dev")
    parser.add_argument("--evidence", type=Path, default=ROOT / ".tmp/stage11/native")
    args = parser.parse_args()
    require_macos()
    require_closed_editor()
    evidence = args.evidence.resolve()
    evidence.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.setdefault("CARGO_INCREMENTAL", "0")
    env.setdefault("CARGO_PROFILE_DEV_DEBUG", "0")
    env.setdefault("CARGO_PROFILE_TEST_DEBUG", "0")
    metadata = json.loads(run([args.cargo, "metadata", "--locked", "--no-deps",
                               "--format-version", "1"], env=env))
    command = [args.cargo, "build", "--locked", "-p", "kitu-demo-game-native",
               "--target", "aarch64-apple-darwin", "--profile", args.profile]
    run(command, log=evidence / "cargo-build.log", env=env)
    profile = "debug" if args.profile == "dev" else "release"
    source = Path(metadata["target_directory"]) / "aarch64-apple-darwin" / profile / LIBRARY
    if not source.is_file():
        raise RuntimeError(f"Cargo did not produce the expected library: {source}")
    # Keep the previous working plugin until relocation and signing both succeed.
    PLUGIN.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=".kitu-plugin-", dir=PLUGIN.parent) as temporary:
        staged = Path(temporary) / LIBRARY
        shutil.copyfile(source, staged)
        staged.chmod(0o755)
        run(["xcrun", "install_name_tool", "-id", INSTALL_NAME, staged])
        run(["/usr/bin/codesign", "--force", "--sign", "-", "--timestamp=none", staged])
        verify_plugin(staged)
        require_closed_editor()
        os.replace(staged, PLUGIN)
    report = {"createdAtUtc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
              "target": "aarch64-apple-darwin", "profile": args.profile,
              "cargoCommand": command, "cargo": run([args.cargo, "--version"]),
              "rustc": run([env.get("RUSTC", "rustc"), "--version", "--verbose"]),
              "sdk": run(["xcrun", "--show-sdk-version"]),
              "sdkPath": run(["xcrun", "--show-sdk-path"]),
              "cargoLock": artifact(ROOT / "Cargo.lock"),
              "sourceLibrary": artifact(source), "plugin": verify_plugin(PLUGIN)}
    write_json(evidence / "native-package.json", report)
    print(json.dumps({"plugin": str(PLUGIN), "evidence": str(evidence / "native-package.json")}, indent=2))


if __name__ == "__main__":
    main()
