"""Shared, dependency-free helpers for the Arena macOS build tools."""

import hashlib
import json
import os
from pathlib import Path
import platform
import shlex
import shutil
import signal
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
PROJECT = ROOT / "kitu-integration-runner/unity-demo-game/kitu-unity-demo-game"
LIBRARY = "libkitu_demo_game_native.dylib"
PLUGIN = PROJECT / "Assets/Plugins/macOS" / LIBRARY
INSTALL_NAME = "@rpath/" + LIBRARY
SYMBOLS = {
    "kitu_application_abi_version", "kitu_application_create",
    "kitu_application_destroy", "kitu_application_submit_json",
    "kitu_application_tick", "kitu_application_read_output",
    "kitu_application_inspect_json", "kitu_application_inspect_host_json",
    "kitu_application_last_error",
}


def run(arguments, *, log=None, env=None, timeout=None, cwd=ROOT):
    """Run one argv command; a timeout terminates only its own process group."""
    arguments = [str(value) for value in arguments]
    if log is None:
        result = subprocess.run(arguments, cwd=cwd, env=env, text=True,
                                capture_output=True, timeout=timeout)
        if result.returncode:
            raise RuntimeError(f"{shlex.join(arguments)} failed ({result.returncode}):\n"
                               f"{result.stdout}{result.stderr}")
        return result.stdout.strip()
    log = Path(log)
    log.parent.mkdir(parents=True, exist_ok=True)
    print(shlex.join(arguments), flush=True)
    with log.open("a") as output:
        output.write("$ " + shlex.join(arguments) + "\n")
        output.flush()
        process = subprocess.Popen(arguments, cwd=cwd, env=env, stdout=output,
                                   stderr=subprocess.STDOUT, start_new_session=True)
        print(f"Process {process.pid}; log: {log}", flush=True)
        try:
            status = process.wait(timeout=timeout)
        except (subprocess.TimeoutExpired, KeyboardInterrupt):
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
            finally:
                # The group can outlive its leader when a child ignores SIGTERM.
                # Always terminate surviving descendants, even after wait succeeds.
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                process.wait()
            raise
    if status:
        raise RuntimeError(f"Command exited {status}; see {log}")


def require_macos():
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("This build targets an Apple Silicon macOS host.")


def require_closed_editor():
    lock = PROJECT / "Temp/UnityLockfile"
    if lock.exists():
        holders = subprocess.run(["/usr/sbin/lsof", "-t", str(lock)],
                                 capture_output=True, text=True)
        if holders.returncode == 0 and holders.stdout.strip():
            raise RuntimeError(f"Close the Editor for {PROJECT} before replacing its plugin "
                               f"or building; active PID(s): {holders.stdout.strip()}")


def sha256(path):
    digest = hashlib.sha256()
    with Path(path).open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def artifact(path):
    path = Path(path).resolve()
    return {"path": str(path), "bytes": path.stat().st_size, "sha256": sha256(path)}


def write_json(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def unsigned_sha256(path):
    """Compare code after removing signatures only from temporary copies."""
    with tempfile.TemporaryDirectory(prefix="kitu-native-signature-") as temporary:
        copy = Path(temporary) / LIBRARY
        shutil.copyfile(path, copy)
        run(["/usr/bin/codesign", "--remove-signature", copy])
        return sha256(copy)


def verify_plugin(path):
    path = Path(path)
    architectures = run(["xcrun", "lipo", "-archs", path]).split()
    if architectures != ["arm64"]:
        raise RuntimeError(f"Expected an arm64 plugin, found {architectures}")
    identity = run(["otool", "-D", path]).splitlines()[1:]
    if identity != [INSTALL_NAME]:
        raise RuntimeError(f"Plugin install name must be {INSTALL_NAME}: {identity}")
    dependencies = [line.strip().split(" (", 1)[0]
                    for line in run(["otool", "-L", path]).splitlines()[1:]]
    for dependency in dependencies:
        if dependency != INSTALL_NAME and not dependency.startswith(("/usr/lib/", "/System/Library/")):
            raise RuntimeError(f"Plugin requires an unbundled dependency: {dependency}")
    exported = {line.split()[-1].removeprefix("_")
                for line in run(["nm", "-gU", path]).splitlines() if line.split()}
    if not SYMBOLS.issubset(exported):
        raise RuntimeError(f"Missing application ABI symbols: {sorted(SYMBOLS - exported)}")
    run(["/usr/bin/codesign", "--verify", "--strict", path])
    return {**artifact(path), "architecture": "arm64", "installName": INSTALL_NAME,
            "dependencies": dependencies, "applicationSymbols": sorted(SYMBOLS),
            "unsignedSha256": unsigned_sha256(path), "signatureVerified": True}
