//! Replay identity from a pre-resolved graph; never invoke Cargo from a build script.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    env, fs,
    path::{Component, Path, PathBuf},
};

fn fail(message: impl std::fmt::Display) -> ! {
    panic!("invalid replay build inputs: {message}; run tools/prepare-kitu-build.py with the same target/features before Cargo");
}
fn string<'a>(value: &'a Value, key: &str) -> &'a str {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_else(|| fail(format!("missing string {key}")))
}
fn array<'a>(value: &'a Value, key: &str) -> &'a Vec<Value> {
    value
        .get(key)
        .and_then(Value::as_array)
        .unwrap_or_else(|| fail(format!("missing array {key}")))
}
fn read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|error| fail(format!("{}: {error}", path.display())))
}
fn canonical(path: &Path) -> PathBuf {
    path.canonicalize()
        .unwrap_or_else(|error| fail(format!("{}: {error}", path.display())))
}
fn watch(path: &Path) {
    println!("cargo:rerun-if-changed={}", path.display());
}
fn field(hash: &mut Sha256, name: &str, bytes: &[u8]) {
    hash.update((name.len() as u64).to_le_bytes());
    hash.update(name.as_bytes());
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
fn collect(root: &Path, path: &Path, paths: &mut BTreeSet<PathBuf>) {
    watch(path);
    let Ok(metadata) = fs::symlink_metadata(path) else {
        if path.exists() {
            fail(format!("cannot inspect {}", path.display()));
        }
        return;
    };
    if metadata.file_type().is_symlink() {
        fail(format!("symbolic source input {}", path.display()));
    }
    if !canonical(path).starts_with(root) {
        fail("source input escapes its declared root");
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path).unwrap_or_else(|error| fail(error)) {
            collect(
                root,
                &entry.unwrap_or_else(|error| fail(error)).path(),
                paths,
            );
        }
    } else if metadata.is_file() {
        paths.insert(path.to_owned());
    } else {
        fail(format!("non-file source input {}", path.display()));
    }
}

/// Kept separate from Cargo environment collection so regression tests exercise
/// exactly the production digest and validation, including fresh source reads.
pub fn identity(
    app: &Path,
    target: &str,
    features: &BTreeSet<String>,
    settings: &[(String, String)],
    compiler: &[u8],
) -> String {
    let app = canonical(app);
    let manifest_path = app.join("kitu-build-inputs.json");
    watch(&manifest_path);
    let manifest: Value =
        serde_json::from_slice(&read(&manifest_path)).unwrap_or_else(|error| fail(error));
    if manifest.get("schema").and_then(Value::as_u64) != Some(1) {
        fail("unsupported schema");
    }
    if canonical(Path::new(string(&manifest, "app_root"))) != app {
        fail("input map belongs to another application root");
    }
    if string(&manifest, "target") != target {
        fail("prepared target does not match Cargo TARGET");
    }
    let prepared_features: BTreeSet<String> = array(&manifest, "features")
        .iter()
        .map(|feature| {
            feature
                .as_str()
                .unwrap_or_else(|| fail("invalid feature"))
                .replace('-', "_")
                .to_uppercase()
        })
        .collect();
    if &prepared_features != features {
        fail("prepared application features do not match Cargo features");
    }
    let guards = array(&manifest, "guards");
    if guards.is_empty() {
        fail("missing dependency resolution guards");
    }
    let mut guarded = BTreeSet::new();
    for guard in guards {
        let path = Path::new(string(guard, "path"));
        watch(path);
        if !guarded.insert(path.to_owned()) {
            fail("duplicate resolution guard");
        }
        match guard.get("sha256") {
            Some(Value::Null) if !path.exists() => {}
            Some(Value::String(expected))
                if hex::encode(Sha256::digest(read(path))) == *expected => {}
            _ => fail(format!(
                "dependency resolution changed at {}",
                path.display()
            )),
        }
    }
    if !guarded.contains(&app.join("Cargo.toml"))
        || !guarded
            .iter()
            .any(|path| path.file_name().is_some_and(|name| name == "Cargo.lock"))
    {
        fail("missing application manifest or lockfile guard");
    }
    let mut hash = Sha256::new();
    field(&mut hash, "identity-format", b"kitu-arena-resolved-v1");
    field(&mut hash, "compiler", compiler);
    field(&mut hash, "target", target.as_bytes());
    for (key, value) in settings {
        field(&mut hash, key, value.as_bytes());
    }
    let graph = array(&manifest, "graph");
    if graph.is_empty() {
        fail("empty resolved graph");
    }
    let mut graph_keys = BTreeSet::new();
    let mut expected_roots = BTreeSet::new();
    for package in graph {
        let key = string(package, "package");
        if !graph_keys.insert(key.to_owned()) {
            fail("duplicate resolved package");
        }
        let source = key
            .splitn(3, '|')
            .nth(2)
            .unwrap_or_else(|| fail("invalid resolved package identity"));
        if source == "path" || source.starts_with("git+") {
            expected_roots.insert(key.to_owned());
        }
        array(package, "features");
        array(package, "dependencies");
    }
    for package in graph {
        for edge in array(package, "dependencies") {
            if !graph_keys.contains(string(edge, "package")) {
                fail("dependency edge outside the resolved graph");
            }
        }
    }
    field(
        &mut hash,
        "resolved-graph",
        &serde_json::to_vec(graph).unwrap(),
    );
    let configuration = manifest
        .get("build_configuration")
        .filter(|value| value.is_object())
        .unwrap_or_else(|| fail("missing build configuration"));
    field(
        &mut hash,
        "build-configuration",
        &serde_json::to_vec(configuration).unwrap(),
    );
    let mut actual_roots = BTreeSet::new();
    let mut found_app = false;
    for source in array(&manifest, "roots") {
        let logical = string(source, "logical");
        if !actual_roots.insert(logical.to_owned()) {
            fail("duplicate source root");
        }
        let root = canonical(Path::new(string(source, "path")));
        found_app |= root == app;
        let cargo_manifest = root.join("Cargo.toml");
        if !guarded.contains(&cargo_manifest) {
            fail("unguarded source package manifest");
        }
        if hex::encode(Sha256::digest(read(&cargo_manifest))) != string(source, "manifest_sha256") {
            fail(format!("source package mismatch at {}", root.display()));
        }
        if !root.join("src").is_dir() {
            fail(format!(
                "missing package source directory at {}",
                root.display()
            ));
        }
        let inputs = array(source, "inputs");
        if !inputs.iter().any(|v| v.as_str() == Some("src"))
            || !inputs.iter().any(|v| v.as_str() == Some("Cargo.toml"))
        {
            fail("source root omits required inputs");
        }
        let mut paths = BTreeSet::new();
        for input in inputs {
            let relative = Path::new(
                input
                    .as_str()
                    .unwrap_or_else(|| fail("invalid source input")),
            );
            if relative.as_os_str().is_empty()
                || relative
                    .components()
                    .any(|part| !matches!(part, Component::Normal(_)))
            {
                fail("source input must be a relative path within its root");
            }
            collect(&root, &root.join(relative), &mut paths);
        }
        for path in paths {
            let relative = path
                .strip_prefix(&root)
                .unwrap()
                .to_str()
                .unwrap_or_else(|| fail("non-UTF8 source path"))
                .replace('\\', "/");
            field(&mut hash, &format!("{logical}/{relative}"), &read(&path));
        }
    }
    if !found_app || actual_roots != expected_roots {
        fail("source roots do not match resolved dependencies");
    }
    hex::encode(hash.finalize())
}

pub fn emit() {
    let app = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let target = env::var("TARGET").unwrap();
    let features = env::vars()
        .filter_map(|(key, _)| key.strip_prefix("CARGO_FEATURE_").map(str::to_owned))
        .collect();
    let mut settings = [
        "PROFILE",
        "OPT_LEVEL",
        "DEBUG",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_CFG_TARGET_FEATURE",
        "CARGO_CFG_PANIC",
        "RUSTC_BOOTSTRAP",
        "RUSTC_WRAPPER",
        "RUSTC_WORKSPACE_WRAPPER",
    ]
    .iter()
    .map(|key| {
        println!("cargo:rerun-if-env-changed={key}");
        (key.to_string(), env::var(key).unwrap_or_default())
    })
    .collect::<Vec<_>>();
    // Cargo profile overrides affect code generation but are not all reflected
    // in OPT_LEVEL/DEBUG. Watch even absent standard-profile overrides.
    let mut profile_keys = BTreeSet::new();
    for profile in ["DEV", "RELEASE", "TEST", "BENCH"] {
        for field in [
            "OPT_LEVEL",
            "DEBUG",
            "SPLIT_DEBUGINFO",
            "STRIP",
            "DEBUG_ASSERTIONS",
            "OVERFLOW_CHECKS",
            "LTO",
            "PANIC",
            "INCREMENTAL",
            "CODEGEN_UNITS",
            "RPATH",
        ] {
            profile_keys.insert(format!("CARGO_PROFILE_{profile}_{field}"));
        }
    }
    profile_keys.extend(
        env::vars().filter_map(|(key, _)| key.starts_with("CARGO_PROFILE_").then_some(key)),
    );
    for key in profile_keys {
        println!("cargo:rerun-if-env-changed={key}");
        settings.push((key.clone(), env::var(&key).unwrap_or_default()));
    }
    println!("cargo:rerun-if-env-changed=RUSTC");
    let compiler = std::process::Command::new(env::var("RUSTC").unwrap())
        .arg("-vV")
        .output()
        .expect("read Rust compiler version");
    assert!(compiler.status.success(), "read Rust compiler version");
    let hash = identity(&app, &target, &features, &settings, &compiler.stdout);
    println!("cargo:rustc-env=ARENA_EXECUTION_HASH={hash}");
    println!("cargo:rustc-env=ARENA_EXECUTION_TARGET={target}");
}
