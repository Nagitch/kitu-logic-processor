#!/usr/bin/env python3
"""Resolve replay identity inputs before Cargo compiles the application.

Run from the same working directory and Cargo environment, with the same target
and feature selection, as the following cargo build/test. Keep patch/profile
configuration in Cargo.toml or .cargo/config.toml so freshness can be checked.
Cargo metadata is deliberately invoked here, never recursively from build.rs.
The generated file contains local paths and must not be committed or distributed.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

SCHEMA = 1
OUTPUT = "kitu-build-inputs.json"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def cargo_document(path):
    return tomllib.loads(path.read_text(encoding="utf-8"))


def workspace_for(manifest):
    for directory in (manifest.parent, *manifest.parent.parents):
        candidate = directory / "Cargo.toml"
        if candidate.is_file() and "workspace" in cargo_document(candidate):
            return candidate
    return manifest


def reachable(metadata, app):
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    pending, seen = [app["id"]], set()
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        if current not in nodes:
            raise ValueError(f"resolved graph is missing {current}")
        seen.add(current)
        pending.extend(edge["pkg"] for edge in nodes[current]["deps"])
    return nodes, seen


def prepare(metadata, manifest, target):
    manifest = manifest.resolve(strict=True)
    packages = {p["id"]: p for p in metadata["packages"]}
    apps = [p for p in packages.values()
            if Path(p["manifest_path"]).resolve() == manifest]
    if len(apps) != 1:
        raise ValueError("metadata must resolve exactly the requested application manifest")
    app = apps[0]
    nodes, selected = reachable(metadata, app)
    selected_packages = [packages[key] for key in selected]
    kitu = [p for p in selected_packages
            if p["name"].startswith("kitu-") and p["id"] != app["id"]]
    if not kitu:
        raise ValueError("no actual Kitu dependencies were resolved")
    origins = {(p.get("source") or "path",
                str(workspace_for(Path(p["manifest_path"]).resolve()).parent)) for p in kitu}
    if len(origins) != 1:
        raise ValueError("mixed Kitu source roots or revisions; substitute the complete Kitu dependency set")
    if any((p.get("source") or "").startswith("registry+") for p in kitu):
        raise ValueError("Kitu replay identity requires inspectable git or path sources")
    if len({p["name"] for p in kitu}) != len(kitu):
        raise ValueError("multiple resolved versions of a Kitu package")

    # Paths never enter the identity. Cargo package IDs for path packages include
    # absolute locations, so replace IDs everywhere, including dependency edges.
    def key(p):
        return "|".join((p["name"], p["version"], p.get("source") or "path"))
    keys = {p["id"]: key(p) for p in selected_packages}
    if len(set(keys.values())) != len(keys):
        raise ValueError("ambiguous package identity from different local source roots")
    lock_path = Path(metadata["workspace_root"]).resolve() / "Cargo.lock"
    if not lock_path.is_file():
        raise ValueError(f"locked dependency resolution requires {lock_path}")
    lock = cargo_document(lock_path)
    locked = {(p["name"], p["version"], p.get("source")): p for p in lock.get("package", [])}
    graph = []
    for p in sorted(selected_packages, key=key):
        node = nodes[p["id"]]
        entry = {"package": key(p), "features": sorted(node["features"]), "dependencies": []}
        lock_entry = locked.get((p["name"], p["version"], p.get("source")))
        if lock_entry is None:
            raise ValueError(f"resolved package missing from Cargo.lock: {key(p)}")
        entry["checksum"] = lock_entry.get("checksum")
        for edge in node["deps"]:
            entry["dependencies"].append({"name": edge["name"], "package": keys[edge["pkg"]],
                "kinds": sorted(edge["dep_kinds"], key=lambda value: json.dumps(value, sort_keys=True))})
        entry["dependencies"].sort(key=lambda edge: (edge["name"], edge["package"]))
        graph.append(entry)

    guards = {}
    def guard(path):
        path = path.resolve()
        guards[str(path)] = digest(path) if path.is_file() else None

    guard(lock_path)
    guard(manifest)
    guard(Path(metadata["workspace_root"]) / "Cargo.toml")
    roots = []
    for p in sorted(selected_packages, key=key):
        path = Path(p["manifest_path"]).resolve(strict=True)
        guard(path)
        guard(workspace_for(path))
        if (p.get("source") or "").startswith(("registry+", "sparse+")):
            continue
        # Directory watches let build.rs discover newly added/removed source
        # files without needing to regenerate the resolved graph.
        inputs = ["Cargo.toml", "src", "build.rs"]
        for cargo_target in p.get("targets", []):
            source = Path(cargo_target["src_path"]).resolve()
            if not source.is_relative_to(path.parent):
                raise ValueError(f"target source escapes its package: {source}")
            relative = source.relative_to(path.parent).as_posix()
            if not relative.startswith("src/"):
                inputs.append(relative)
        if p["id"] == app["id"]:
            inputs += ["content", "kitu-app-actions.toml", "build_identity.rs"]
        roots.append({"logical": key(p), "path": str(path.parent), "manifest_sha256": digest(path),
                      "inputs": sorted(set(inputs))})

    # Cargo reads configuration from invocation cwd upwards, not manifest-path.
    # Watching missing configs also catches configuration created after prepare.
    for directory in (Path.cwd(), *Path.cwd().parents):
        for filename in ("config", "config.toml"):
            guard(directory / ".cargo" / filename)
    cargo_home = Path(os.environ.get("CARGO_HOME", str(Path.home() / ".cargo")))
    config_paths = []
    for directory in [cargo_home, *(directory / ".cargo" for directory in reversed((Path.cwd(), *Path.cwd().parents)))]:
        for filename in ("config", "config.toml"):
            candidate = (directory / filename).resolve()
            guard(candidate)
            if candidate.is_file() and candidate not in config_paths:
                config_paths.append(candidate)
    configurations = [cargo_document(path) for path in config_paths]
    return {"schema": SCHEMA, "app_root": str(manifest.parent), "target": target,
            "features": sorted(nodes[app["id"]]["features"]), "graph": graph,
            "build_configuration": {"profiles": cargo_document(Path(metadata["workspace_root"]) / "Cargo.toml").get("profile", {}),
                                    "cargo_configs": configurations},
            "roots": roots, "guards": [{"path": path, "sha256": value}
                for path, value in sorted(guards.items())]}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest-path", required=True, type=Path)
    parser.add_argument("--target")
    parser.add_argument("--cargo", default=os.environ.get("CARGO", "cargo"))
    parser.add_argument("--features", action="append", default=[])
    parser.add_argument("--all-features", action="store_true")
    parser.add_argument("--no-default-features", action="store_true")
    args = parser.parse_args(argv)
    try:
        manifest = args.manifest_path.resolve(strict=True)
        target = args.target or os.environ.get("CARGO_BUILD_TARGET")
        if not target:
            version = subprocess.check_output([os.environ.get("RUSTC", "rustc"), "-vV"], text=True)
            target = next(line.removeprefix("host: ") for line in version.splitlines() if line.startswith("host: "))
        command = [args.cargo, "metadata", "--locked", "--format-version", "1",
                   "--manifest-path", str(manifest), "--filter-platform", target]
        for feature in args.features:
            command += ["--features", feature]
        if args.all_features:
            command.append("--all-features")
        if args.no_default_features:
            command.append("--no-default-features")
        metadata = json.loads(subprocess.check_output(command, text=True))
        result = prepare(metadata, manifest, target)
        output = manifest.parent / OUTPUT
        temporary = output.with_suffix(".json.tmp")
        temporary.write_text(json.dumps(result, sort_keys=True, indent=2) + "\n", encoding="utf-8")
        temporary.replace(output)
        print(f"Prepared replay build inputs: {output}")
        return 0
    except (OSError, ValueError, KeyError, StopIteration, subprocess.CalledProcessError) as error:
        print(f"Cannot prepare replay build inputs: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
