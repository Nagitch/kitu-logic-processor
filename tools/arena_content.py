"""Stage and inspect the fixed Arena source package without interpreting game rules."""

import hashlib
import json
from pathlib import Path
import re
import tempfile

FILES = (
    ("unity-assets.json", 8 * 1024),
    ("arena.tmd", 128 * 1024),
    ("boss.rhai", 64 * 1024),
    ("timelines/boss-telegraph.tsq", 8 * 1024),
    ("timelines/floor-transition.tsq", 8 * 1024),
)
ROLES = (("baseMaterial", "Material"), ("cube", "GameObject"),
         ("capsule", "GameObject"), ("sphere", "GameObject"))
MANIFEST_LIMIT = 16 * 1024


def digest(value):
    return hashlib.sha256(value).hexdigest()


def object_fields(pairs):
    result = {}
    for name, value in pairs:
        if name in result:
            raise ValueError(f"Duplicate JSON field: {name}")
        result[name] = value
    return result


def read_json(data):
    return json.loads(data.decode("utf-8"), object_pairs_hook=object_fields,
                      parse_constant=invalid_constant)


def invalid_constant(value):
    raise ValueError(f"Non-JSON number: {value}")


def fields(value, names, label):
    if not isinstance(value, dict) or set(value) != set(names):
        raise ValueError(f"{label} requires exactly {', '.join(names)}")


def bounded(path, maximum):
    path = Path(path)
    if path.is_symlink() or not path.is_file():
        raise ValueError(f"Packaged sources must be regular files: {path}")
    with path.open("rb") as source:
        data = source.read(maximum + 1)
    if not data or len(data) > maximum:
        raise ValueError(f"{path.name} must contain 1..{maximum} bytes")
    return data


def source_file(directory, name):
    path = directory / name
    for component in (path, *path.parents):
        if component == directory:
            break
        if component.is_symlink():
            raise ValueError(f"Packaged source cannot use a symlink: {component}")
    return path


def visual_mapping(data):
    mapping = read_json(data)
    fields(mapping, ("schemaVersion", "assets"), "Unity asset mapping")
    if type(mapping["schemaVersion"]) is not int or mapping["schemaVersion"] != 1:
        raise ValueError("Unsupported Unity asset mapping schema")
    assets = mapping["assets"]
    if not isinstance(assets, list) or len(assets) != len(ROLES):
        raise ValueError("Unity asset mapping requires four ordered roles")
    keys = set()
    for asset, (role, kind) in zip(assets, ROLES):
        fields(asset, ("role", "key", "type"), "Unity asset entry")
        key = asset["key"]
        if asset["role"] != role or asset["type"] != kind:
            raise ValueError(f"Expected {role} with type {kind}")
        if not isinstance(key, str) or not key or "\0" in key or len(key.encode("utf-8")) > 128:
            raise ValueError(f"Invalid Addressable key for {role}")
        if key in keys:
            raise ValueError(f"Duplicate Addressable key: {key}")
        keys.add(key)
    return assets


def inspect_package(directory):
    if Path(directory).is_symlink():
        raise ValueError("Arena package directory cannot be a symlink")
    directory = Path(directory).resolve()
    manifest_bytes = bounded(source_file(directory, "package.json"), MANIFEST_LIMIT)
    manifest = read_json(manifest_bytes)
    fields(manifest, ("schemaVersion", "files"), "Arena package")
    if type(manifest["schemaVersion"]) is not int or manifest["schemaVersion"] != 1:
        raise ValueError("Unsupported Arena package schema")
    if not isinstance(manifest["files"], list) or len(manifest["files"]) != len(FILES):
        raise ValueError("Arena package requires the five ordered files")
    assets = None
    for entry, (name, maximum) in zip(manifest["files"], FILES):
        fields(entry, ("path", "bytes", "sha256"), "Arena package file")
        if entry["path"] != name:
            raise ValueError(f"Expected packaged path {name}")
        if type(entry["bytes"]) is not int or not 0 < entry["bytes"] <= maximum:
            raise ValueError(f"Invalid packaged byte count for {name}")
        if not isinstance(entry["sha256"], str) or not re.fullmatch("[0-9a-f]{64}", entry["sha256"]):
            raise ValueError(f"Invalid packaged digest for {name}")
        data = bounded(source_file(directory, name), maximum)
        if len(data) != entry["bytes"] or digest(data) != entry["sha256"]:
            raise ValueError(f"Packaged source does not match its manifest: {name}")
        if name == "unity-assets.json":
            assets = visual_mapping(data)
    return {"directory": str(directory), "hash": digest(manifest_bytes),
            "manifestBytes": len(manifest_bytes), "files": manifest["files"],
            "assets": assets}


def stage_package(source, destination):
    """Read every source before replacing only known generated destination files."""
    if Path(source).is_symlink() or Path(destination).is_symlink():
        raise ValueError("Package source and staging directories cannot be symlinks")
    source, destination = Path(source).resolve(), Path(destination).resolve()
    if source == destination:
        raise ValueError("Package source and staging directory must differ")
    payloads = [(name, bounded(source_file(source, name), maximum)) for name, maximum in FILES]
    visual_mapping(payloads[0][1])
    manifest = {"schemaVersion": 1, "files": [
        {"path": name, "bytes": len(data), "sha256": digest(data)}
        for name, data in payloads]}
    encoded = (json.dumps(manifest, ensure_ascii=False, separators=(",", ":")) + "\n").encode("utf-8")
    # The manifest commits the generation last. An interrupted staging operation
    # is rejected by readers instead of silently combining two source versions.
    for name, data in [*payloads, ("package.json", encoded)]:
        path = source_file(destination, name)
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists() and path.read_bytes() == data:
            continue
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=path.parent, prefix=path.name + ".", delete=False) as output:
                temporary = Path(output.name)
                output.write(data)
            temporary.replace(path)
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)
    return inspect_package(destination)


def relative_file(directory, name):
    """Accept only a build-reported local path beneath the artifact root."""
    if not isinstance(name, str) or not name or "\\" in name:
        raise ValueError("Invalid local Addressables artifact path")
    path = Path(name)
    if path.is_absolute() or any(part in (".", "..") for part in path.parts):
        raise ValueError(f"Nonlocal Addressables artifact path: {name}")
    return source_file(directory, name)


def verify_player_content(player, build, source):
    """Check actual Player files against the packed build and package identities."""
    player = Path(player).resolve()
    streaming = list(player.rglob("StreamingAssets"))
    if len(streaming) != 1 or not streaming[0].is_dir():
        raise ValueError("Player must contain exactly one StreamingAssets directory")
    installed = inspect_package(streaming[0] / "KituArena")
    if installed["hash"] != source["hash"] or installed["hash"] != build["package"]["identity"]:
        raise ValueError("Player source package differs from the reviewed build input")
    if build["packageVersion"] != "2.11.2" or build["remoteCatalog"] is not False or build["builder"] != "BuildScriptPackedMode":
        raise ValueError("Player did not use the required local packed Addressables build")
    root = streaming[0] / "aa"
    if root.is_symlink() or not root.is_dir():
        raise ValueError("Player Addressables directory must be local")
    expected, verified = set(), []
    for entry in build["artifacts"]:
        name = entry["relativePath"]
        path = relative_file(root, name)
        if name in expected or not path.is_file():
            raise ValueError(f"Missing or duplicate bundled Addressables artifact: {name}")
        expected.add(name)
        data = path.read_bytes()
        if len(data) != entry["bytes"] or digest(data) != entry["sha256"]:
            raise ValueError(f"Bundled Addressables artifact differs from the content build: {name}")
        verified.append({"relativePath": name, "bytes": len(data), "sha256": digest(data)})
    bundles = {path.relative_to(root).as_posix() for path in root.rglob("*.bundle")}
    if not bundles or bundles != {name for name in expected if name.endswith(".bundle")}:
        raise ValueError("Player has missing or unexpected Addressables bundles")
    catalogs = list(root.rglob("catalog*.bin"))
    if len(catalogs) != 1 or digest(catalogs[0].read_bytes()) != build["catalog"]["sha256"]:
        raise ValueError("Player binary catalog does not match the packed content build")
    settings = root / "settings.json"
    if not settings.is_file() or digest(settings.read_bytes()) != build["settings"]["sha256"]:
        raise ValueError("Player Addressables initialization settings changed after content build")
    prefix = "{UnityEngine.AddressableAssets.Addressables.RuntimePath}/"
    locations = build["locations"]
    for location in locations:
        identifier, provider = location["internalId"], location["provider"]
        if not isinstance(identifier, str) or "://" in identifier or identifier.startswith("/"):
            raise ValueError("Addressables catalog contains an external load location")
        if provider == "UnityEngine.ResourceManagement.ResourceProviders.AssetBundleProvider":
            if not identifier.startswith(prefix):
                raise ValueError("Addressables bundle location is outside the Player")
            relative = identifier[len(prefix):]
            relative_file(root, relative)
            if relative not in bundles:
                raise ValueError("Addressables catalog refers to a missing local bundle")
        elif provider != "UnityEngine.ResourceManagement.ResourceProviders.BundledAssetProvider":
            raise ValueError("Addressables catalog uses an unexpected provider")
    for asset in installed["assets"]:
        matches = [entry for entry in locations if entry["key"] == asset["key"]]
        if len(matches) != 1 or matches[0]["type"] != "UnityEngine." + asset["type"]:
            raise ValueError(f"Player catalog is missing the requested key/type: {asset['key']}")
    return {"package": installed, "addressablesVersion": build["packageVersion"],
            "catalogSha256": build["catalog"]["sha256"], "localOnly": True,
            "artifacts": verified, "locations": locations,
            "runtimeUse": "The separate graphical Player probes verify actual loading and use."}
