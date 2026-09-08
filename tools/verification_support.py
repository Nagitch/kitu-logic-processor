"""Logged process groups and artifact identities for framework verification."""

import hashlib
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import threading
from contextlib import contextmanager

ROOT = Path(__file__).resolve().parents[1]


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
