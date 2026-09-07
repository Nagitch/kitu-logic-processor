use super::*;
use crate::{
    arena, build_demo_runtime,
    replay::{Recorder, Session},
};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::InputMetadata;
use kitu_tsq1::presentation::Clip;
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct PackageDir(PathBuf);
impl PackageDir {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "arena-package-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(path.join("timelines")).unwrap();
        let result = Self(path);
        for (path, bytes) in FILE_PATHS.into_iter().zip([
            DEFAULT_UNITY_ASSETS,
            include_bytes!("../../../content/arena.tmd"),
            include_bytes!("../../../content/boss.rhai"),
            arena::presentation::DEFAULT_BOSS,
            arena::presentation::DEFAULT_FLOOR,
        ]) {
            std::fs::write(result.0.join(path), bytes).unwrap();
        }
        result.manifest();
        result
    }
    fn manifest(&self) {
        let files = FILE_PATHS
            .iter()
            .map(|path| {
                let bytes = std::fs::read(self.0.join(path)).unwrap();
                PackageFile {
                    path: path.to_string(),
                    bytes: bytes.len() as u64,
                    sha256: hash(&bytes),
                }
            })
            .collect();
        self.write_manifest(
            serde_json::to_value(PackageManifest {
                schema_version: 1,
                files,
            })
            .unwrap(),
        );
    }
    fn write_manifest(&self, value: Value) {
        std::fs::write(
            self.0.join("package.json"),
            serde_json::to_vec(&value).unwrap(),
        )
        .unwrap();
    }
    fn manifest_value(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.0.join("package.json")).unwrap()).unwrap()
    }
    fn replace(&self, path: &str, bytes: &[u8]) {
        std::fs::write(self.0.join(path), bytes).unwrap();
        self.manifest();
    }
}
impl Drop for PackageDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn default_package_has_exact_sources_and_relocatable_raw_manifest_identity() {
    let directory = PackageDir::new();
    let package = load_package(&directory.0).unwrap();
    let baseline = crate::build_arena_runtime().unwrap();
    assert_eq!(
        package.content.hash,
        arena::inspect_content(&baseline).unwrap().pending.hash
    );
    assert_eq!(
        package.script.hash,
        arena::inspect_script(&baseline).unwrap().pending.hash
    );
    assert_eq!(
        package.timeline.hash,
        arena::inspect_timeline(&baseline).unwrap().pending.hash
    );
    assert_eq!(
        package.identity.hash,
        hash(&std::fs::read(directory.0.join("package.json")).unwrap())
    );
    assert_eq!(
        package.source_bytes("arena.tmd").unwrap(),
        include_bytes!("../../../content/arena.tmd")
    );
    assert!(package.source_bytes("../arena.tmd").is_none());
    let relocated = PackageDir::new();
    assert_eq!(
        package.identity,
        load_package(&relocated.0).unwrap().identity
    );
    let manifest = directory.manifest_value();
    std::fs::write(
        directory.0.join("package.json"),
        serde_json::to_vec_pretty(&manifest).unwrap(),
    )
    .unwrap();
    let whitespace = load_package(&directory.0).unwrap();
    assert_ne!(package.identity.hash, whitespace.identity.hash);
    assert_eq!(package.identity.files, whitespace.identity.files);
    assert_eq!(package.content.hash, whitespace.content.hash);
}

#[test]
fn fixed_manifest_contract_rejects_versions_paths_types_unknowns_and_duplicates() {
    let directory = PackageDir::new();
    let original = directory.manifest_value();
    let mut invalid = Vec::new();
    for (key, value) in [("schemaVersion", json!(2)), ("extra", json!(true))] {
        let mut v = original.clone();
        v[key] = value;
        invalid.push(v);
    }
    for (key, value) in [
        ("path", json!("../arena.tmd")),
        ("bytes", json!(1.0)),
        ("bytes", json!(-1)),
        ("sha256", json!("A".repeat(64))),
        ("extra", json!(0)),
    ] {
        let mut v = original.clone();
        v["files"][0][key] = value;
        invalid.push(v);
    }
    let mut v = original.clone();
    v["files"].as_array_mut().unwrap().swap(0, 1);
    invalid.push(v);
    let mut v = original.clone();
    v["files"].as_array_mut().unwrap().pop();
    invalid.push(v);
    let mut v = original.clone();
    v["files"][0] = json!(["unity-assets.json", 1, "hash"]);
    invalid.push(v);
    invalid.push(json!([1, original["files"]]));
    for value in invalid {
        directory.write_manifest(value.clone());
        assert!(load_package(&directory.0).is_err(), "accepted {value}");
    }
    let original_json = serde_json::to_string(&original).unwrap();
    for bytes in [
        original_json.replacen(
            "\"schemaVersion\":1",
            "\"schemaVersion\":1,\"schemaVersion\":1",
            1,
        ),
        original_json.replacen("\"path\":", "\"path\":\"unity-assets.json\",\"path\":", 1),
        format!("{original_json} true"),
        format!("\u{feff}{original_json}"),
    ] {
        std::fs::write(directory.0.join("package.json"), bytes).unwrap();
        assert!(load_package(&directory.0).is_err());
    }
}

#[test]
fn mapping_allows_data_keys_but_rejects_wrong_types_roles_and_ambiguous_json() {
    let directory = PackageDir::new();
    let original: Value = serde_json::from_slice(DEFAULT_UNITY_ASSETS).unwrap();
    let mut custom = original.clone();
    custom["assets"][1]["key"] = json!("arena/custom/箱");
    directory.replace("unity-assets.json", &serde_json::to_vec(&custom).unwrap());
    assert_eq!(
        load_package(&directory.0).unwrap().assets.assets[1].key,
        "arena/custom/箱"
    );
    let mut invalid = Vec::new();
    for (key, value) in [
        ("key", json!("")),
        ("key", json!("x".repeat(129))),
        ("key", json!("a\0b")),
        ("key", original["assets"][1]["key"].clone()),
        ("role", json!("unknown")),
        ("type", json!("GameObject")),
        ("extra", json!(false)),
    ] {
        let mut v = original.clone();
        v["assets"][0][key] = value;
        invalid.push(v);
    }
    let mut v = original.clone();
    v["schemaVersion"] = json!(2);
    invalid.push(v);
    let mut v = original.clone();
    v["assets"].as_array_mut().unwrap().swap(1, 2);
    invalid.push(v);
    let mut v = original.clone();
    v["assets"][0] = json!(["baseMaterial", "arena/material/base", "Material"]);
    invalid.push(v);
    for value in invalid {
        directory.replace("unity-assets.json", &serde_json::to_vec(&value).unwrap());
        assert!(load_package(&directory.0).is_err(), "accepted {value}");
    }
    let duplicate = std::str::from_utf8(DEFAULT_UNITY_ASSETS).unwrap().replacen(
        "\"role\":",
        "\"role\":\"baseMaterial\",\"role\":",
        1,
    );
    directory.replace("unity-assets.json", duplicate.as_bytes());
    assert!(load_package(&directory.0).is_err());
}

#[test]
fn missing_digest_and_size_failures_never_use_compiled_fallbacks() {
    for path in FILE_PATHS {
        let directory = PackageDir::new();
        let file = directory.0.join(path);
        let mut bytes = std::fs::read(&file).unwrap();
        bytes[0] ^= 1;
        std::fs::write(&file, &bytes).unwrap();
        assert!(load_package(&directory.0)
            .unwrap_err()
            .to_string()
            .contains("SHA256 mismatch"));
        bytes.push(0);
        std::fs::write(&file, bytes).unwrap();
        assert!(load_package(&directory.0)
            .unwrap_err()
            .to_string()
            .contains("byte length mismatch"));
        std::fs::remove_file(file).unwrap();
        assert!(load_package(&directory.0).is_err());
    }
    let directory = PackageDir::new();
    std::fs::write(
        directory.0.join("package.json"),
        vec![b' '; MAX_MANIFEST_BYTES + 1],
    )
    .unwrap();
    assert!(load_package(&directory.0)
        .unwrap_err()
        .to_string()
        .contains("byte limit"));
    for (path, cap) in FILE_PATHS.into_iter().zip(MAX_FILE_BYTES) {
        let directory = PackageDir::new();
        std::fs::write(directory.0.join(path), vec![b' '; cap + 1]).unwrap();
        // Declared size is unchanged: actual file bounds are independently checked.
        assert!(load_package(&directory.0)
            .unwrap_err()
            .to_string()
            .contains("byte limit"));
        directory.manifest();
        assert!(load_package(&directory.0)
            .unwrap_err()
            .to_string()
            .contains("byte limit"));
    }
}

#[test]
fn correctly_hashed_but_invalid_domain_sources_are_rejected() {
    for (path, bytes, context) in [
        ("arena.tmd", b"not a TMD".as_slice(), "packaged TMD"),
        ("boss.rhai", b"fn boss(input) {".as_slice(), "packaged Rhai"),
        (
            "timelines/boss-telegraph.tsq",
            b"not TSQ1".as_slice(),
            "packaged TSQ1",
        ),
        (
            "timelines/floor-transition.tsq",
            b"not TSQ1".as_slice(),
            "packaged TSQ1",
        ),
    ] {
        let directory = PackageDir::new();
        directory.replace(path, bytes);
        assert!(load_package(&directory.0)
            .unwrap_err()
            .to_string()
            .contains(context));
    }
    let directory = PackageDir::new();
    directory.replace("boss.rhai", &[255]);
    assert!(load_package(&directory.0)
        .unwrap_err()
        .to_string()
        .contains("UTF-8"));
}

#[cfg(unix)]
#[test]
fn non_regular_sources_and_symlink_components_fail_without_opening_them() {
    use std::os::unix::fs::symlink;
    assert!(load_package(Path::new("relative")).is_err());
    let directory = PackageDir::new();
    let link = directory.0.with_extension("symlink");
    symlink(&directory.0, &link).unwrap();
    assert!(load_package(&link).is_err());
    std::fs::remove_file(link).unwrap();
    let external = PackageDir::new();
    for path in ["package.json", "arena.tmd"] {
        let directory = PackageDir::new();
        let file = directory.0.join(path);
        std::fs::remove_file(&file).unwrap();
        symlink(external.0.join(path), &file).unwrap();
        assert!(load_package(&directory.0).is_err());
    }
    let directory = PackageDir::new();
    std::fs::remove_dir_all(directory.0.join("timelines")).unwrap();
    symlink(external.0.join("timelines"), directory.0.join("timelines")).unwrap();
    assert!(load_package(&directory.0).is_err());
    let directory = PackageDir::new();
    let file = directory.0.join("boss.rhai");
    std::fs::remove_file(&file).unwrap();
    let name = std::ffi::CString::new(file.as_os_str().as_encoded_bytes()).unwrap();
    assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
    assert!(load_package(&directory.0).is_err());
}

#[test]
fn edited_package_is_detached_and_replay_survives_source_replacement_and_deletion() {
    let directory = PackageDir::new();
    let mut values = arena::config::ArenaConfig::default();
    values
        .items
        .iter_mut()
        .find(|item| item.id == "starter")
        .unwrap()
        .damage = 32;
    directory.replace("arena.tmd", &values.to_tmd().unwrap());
    let source = std::str::from_utf8(include_bytes!("../../../content/boss.rhai"))
        .unwrap()
        .replace("duration: 0.8", "duration: 1.6");
    directory.replace("boss.rhai", source.as_bytes());
    let mut boss = Clip::decode(arena::presentation::DEFAULT_BOSS).unwrap();
    for event in &mut boss.events {
        event.bundle.messages[0].args[0] = OscArg::Float(4.5);
    }
    directory.replace("timelines/boss-telegraph.tsq", &boss.encode().unwrap());
    let package = load_package(&directory.0).unwrap();
    assert_eq!(package.content.values, values);
    let mut runtime = build_demo_runtime().unwrap();
    arena::install_with_all_versions(
        &mut runtime,
        package.content.clone(),
        package.script.clone(),
        package.timeline.clone(),
    )
    .unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    let mut bundle = OscBundle::new();
    bundle.push(OscMessage::new("/input/arena/start"));
    runtime
        .try_enqueue_input(
            bundle,
            Some(InputMetadata {
                source: "package-test".into(),
                message_id: 1,
                schema_version: 1,
            }),
        )
        .unwrap();
    for _ in 0..20 {
        runtime.tick_once().unwrap();
        let outputs = runtime.drain_output_buffer();
        recorder.capture(&runtime, &outputs).unwrap();
    }
    directory.replace("boss.rhai", include_bytes!("../../../content/boss.rhai"));
    std::fs::remove_dir_all(&directory.0).unwrap();
    assert_eq!(
        package.source_bytes("boss.rhai").unwrap(),
        source.as_bytes()
    );
    let session = Session::decode(&recorder.encode().unwrap()).unwrap();
    assert_eq!(
        session.manifest().initial_content.hash,
        package.identity.content_hash
    );
    assert_eq!(
        session.manifest().initial_script.hash,
        package.identity.script_hash
    );
    assert_eq!(
        session.manifest().initial_timeline.hash,
        package.identity.timeline_hash
    );
    assert_eq!(session.verify().unwrap().ticks, 20);
}
