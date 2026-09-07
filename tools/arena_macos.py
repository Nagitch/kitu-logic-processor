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
import threading
from contextlib import contextmanager


@contextmanager
def termination_guard():
    """Let a Python wrapper clean its owned children when its caller cancels it."""
    if threading.current_thread() is not threading.main_thread():
        yield
        return
    previous = signal.getsignal(signal.SIGTERM)

    def interrupted(_signal, _frame):
        raise KeyboardInterrupt("verification was terminated")

    signal.signal(signal.SIGTERM, interrupted)
    try:
        yield
    finally:
        signal.signal(signal.SIGTERM, previous)


class OwnedProcess:
    """One logged process group; stop never selects a process by name or port."""

    def __init__(self, arguments, *, log, env=None, cwd=None, shutdown_grace=10):
        self.arguments = [str(value) for value in arguments]
        self.log = Path(log)
        self.shutdown_grace = shutdown_grace
        self.log.parent.mkdir(parents=True, exist_ok=True)
        self.output = self.log.open("a")
        self.output.write("$ " + shlex.join(self.arguments) + "\n")
        self.output.flush()
        try:
            self.process = subprocess.Popen(
                self.arguments, cwd=cwd, env=env, stdout=self.output,
                stderr=subprocess.STDOUT, start_new_session=True)
        except BaseException:
            self.output.close()
            raise
        self.closed = False
        print(f"Process {self.process.pid}; log: {self.log}", flush=True)

    def wait(self, timeout=None):
        """Wait with wrapper cancellation forwarding; retain the actual exit code."""
        with termination_guard():
            try:
                return self.process.wait(timeout=timeout)
            except BaseException:
                self.stop()
                raise

    def stop(self, grace=None):
        """TERM then reap/KILL this group, including children after leader exit."""
        if self.closed:
            return self.process.returncode
        if grace is None:
            grace = self.shutdown_grace
        try:
            try:
                os.killpg(self.process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                self.process.wait(timeout=grace)
            except subprocess.TimeoutExpired:
                pass
            finally:
                try:
                    os.killpg(self.process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
                self.process.wait()
        finally:
            self.output.close()
            self.closed = True
        return self.process.returncode

    def __enter__(self):
        return self

    def __exit__(self, *_exception):
        self.stop()

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
    with OwnedProcess(arguments, log=log, env=env, cwd=cwd) as process:
        status = process.wait(timeout=timeout)
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


def clone_tree(source, destination):
    """Copy an owned macOS artifact with APFS cloning, preserving symlinks."""
    source, destination = Path(source), Path(destination)
    if destination.exists() or destination.is_symlink():
        raise ValueError("Artifact clone destination must be absent")
    destination.parent.mkdir(parents=True, exist_ok=True)
    if platform.system() == "Darwin":
        run(["/bin/cp", "-cR", source, destination])
    else:
        # Portable helper tests use ordinary copies; Apple verification requires Darwin.
        shutil.copytree(source, destination, symlinks=True)


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
