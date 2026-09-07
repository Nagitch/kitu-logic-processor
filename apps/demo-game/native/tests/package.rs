//! Package initialization and authoring isolation through the real exported C ABI.
use kitu_demo_game::arena::{
    self,
    package::{load_package, PackageFile, PackageManifest, DEFAULT_UNITY_ASSETS, FILE_PATHS},
};
use kitu_demo_game_native::*;
use kitu_unity_ffi::application::{ApplicationHandle, BUFFER_TOO_SMALL, DRIVER_ERROR, OK};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
};

type Reader = unsafe extern "C" fn(*mut ApplicationHandle, *mut u8, usize, *mut usize) -> i32;
struct Native(*mut ApplicationHandle);
impl Native {
    fn create(config: Value) -> Result<Self, String> {
        let config = serde_json::to_vec(&config).unwrap();
        let (mut handle, mut required) = (ptr::null_mut(), 0);
        let mut error = vec![0; 16384];
        let status = unsafe {
            kitu_application_create(
                1,
                config.as_ptr(),
                config.len(),
                &mut handle,
                error.as_mut_ptr(),
                error.len(),
                &mut required,
            )
        };
        if status != OK {
            assert_eq!(status, DRIVER_ERROR);
            assert!(handle.is_null());
            assert!(required > 0 && required <= error.len());
            return Err(String::from_utf8(error[..required].to_vec()).unwrap());
        }
        assert!(!handle.is_null());
        Ok(Self(handle))
    }
    fn read(&self, reader: Reader) -> Value {
        let mut required = 0;
        assert_eq!(
            unsafe { reader(self.0, ptr::null_mut(), 0, &mut required) },
            BUFFER_TOO_SMALL
        );
        let mut bytes = vec![0; required];
        assert_eq!(
            unsafe { reader(self.0, bytes.as_mut_ptr(), bytes.len(), &mut required) },
            OK
        );
        serde_json::from_slice(&bytes).unwrap()
    }
    fn projection(&self, address: &str) -> Value {
        projection(&self.read(kitu_application_inspect_json), address)
    }
    fn metadata(&self) -> Value {
        projection(
            &self.read(kitu_application_inspect_host_json),
            "/host/arena/status",
        )
    }
    fn start(&self) {
        let request = serde_json::to_vec(&json!({"metadata":{"source":"native-package","messageId":1,"schemaVersion":1},"bundle":{"messages":[{"address":"/input/arena/start","args":[]}]}})).unwrap();
        let mut sequence = u64::MAX;
        assert_eq!(
            unsafe {
                kitu_application_submit_json(self.0, request.as_ptr(), request.len(), &mut sequence)
            },
            OK
        );
        assert_eq!(sequence, 0);
    }
    fn tick(&self) -> Value {
        assert_eq!(unsafe { kitu_application_tick(self.0) }, OK);
        self.read(kitu_application_read_output)
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        assert_eq!(unsafe { kitu_application_destroy(self.0) }, OK);
    }
}
fn projection(bundles: &Value, address: &str) -> Value {
    let message = bundles
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|bundle| bundle["messages"].as_array().unwrap())
        .find(|message| message["address"] == address)
        .unwrap_or_else(|| panic!("missing {address}"));
    serde_json::from_str(message["args"][0]["value"].as_str().unwrap()).unwrap()
}
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "arena-native-package-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn package(edited: bool) -> Self {
        let result = Self::new();
        std::fs::create_dir_all(result.0.join("timelines")).unwrap();
        for (path, bytes) in FILE_PATHS.into_iter().zip([
            DEFAULT_UNITY_ASSETS,
            include_bytes!("../../content/arena.tmd"),
            include_bytes!("../../content/boss.rhai"),
            arena::presentation::DEFAULT_BOSS,
            arena::presentation::DEFAULT_FLOOR,
        ]) {
            std::fs::write(result.0.join(path), bytes).unwrap();
        }
        if edited {
            let mut values = arena::config::ArenaConfig::default();
            values
                .items
                .iter_mut()
                .find(|item| item.id == "starter")
                .unwrap()
                .damage = 32;
            std::fs::write(result.0.join("arena.tmd"), values.to_tmd().unwrap()).unwrap();
            std::fs::write(
                result.0.join("boss.rhai"),
                std::str::from_utf8(include_bytes!("../../content/boss.rhai"))
                    .unwrap()
                    .replace("duration: 0.8", "duration: 1.6"),
            )
            .unwrap();
            let mut clip =
                kitu_tsq1::presentation::Clip::decode(arena::presentation::DEFAULT_BOSS).unwrap();
            for event in &mut clip.events {
                event.bundle.messages[0].args[0] = kitu_osc_ir::OscArg::Float(4.5);
            }
            std::fs::write(
                result.0.join("timelines/boss-telegraph.tsq"),
                clip.encode().unwrap(),
            )
            .unwrap();
        }
        let files = FILE_PATHS
            .iter()
            .map(|path| {
                let bytes = std::fs::read(result.0.join(path)).unwrap();
                PackageFile {
                    path: path.to_string(),
                    bytes: bytes.len() as u64,
                    sha256: Sha256::digest(&bytes)
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                }
            })
            .collect();
        std::fs::write(
            result.0.join("package.json"),
            serde_json::to_vec(&PackageManifest {
                schema_version: 1,
                files,
            })
            .unwrap(),
        )
        .unwrap();
        result
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn packaged_defaults_match_default_abi_initialization_and_all_outputs() {
    let directory = Directory::package(false);
    let package = load_package(&directory.0).unwrap();
    let default = Native::create(json!({})).unwrap();
    let native = Native::create(json!({"bundledContentDirectory":directory.0})).unwrap();
    assert_eq!(default.metadata()["package"], Value::Null);
    assert_eq!(
        native.metadata()["package"],
        serde_json::to_value(package.identity).unwrap()
    );
    assert_eq!(
        default.read(kitu_application_inspect_json),
        native.read(kitu_application_inspect_json)
    );
    // CI always covers the generated fixture. Explicit local evidence also
    // exercises the independent Python packager's exact emitted bytes.
    if let Some(path) = std::env::var_os("KITU_PACKAGE_INTEROP_DIR") {
        let path = PathBuf::from(path);
        let authored = load_package(&path).unwrap();
        let independent = Native::create(json!({"bundledContentDirectory":path})).unwrap();
        assert_eq!(
            independent.read(kitu_application_inspect_json),
            default.read(kitu_application_inspect_json)
        );
        assert_eq!(
            independent.metadata()["package"],
            serde_json::to_value(&authored.identity).unwrap()
        );
        eprintln!(
            "Python package interoperability: {}",
            authored.identity.hash
        );
    }
    default.start();
    native.start();
    for _ in 0..20 {
        assert_eq!(default.tick(), native.tick());
        assert_eq!(
            default.read(kitu_application_inspect_json),
            native.read(kitu_application_inspect_json)
        );
    }
}

#[test]
fn selected_package_seeds_exact_sources_and_existing_authoring_edits_never_replace_initial_versions(
) {
    let directory = Directory::package(true);
    let storage = Directory::new();
    let package = load_package(&directory.0).unwrap();
    let native =
        Native::create(json!({"bundledContentDirectory":directory.0,"storageDirectory":storage.0}))
            .unwrap();
    for path in &FILE_PATHS[1..] {
        assert_eq!(
            std::fs::read(storage.0.join(path)).unwrap(),
            package.source_bytes(path).unwrap()
        );
    }
    assert_eq!(
        native.projection("/ui/arena/content")["pending"]["hash"],
        package.content.hash
    );
    assert_eq!(
        native.projection("/ui/arena/script")["pending"]["hash"],
        package.script.hash
    );
    assert_eq!(
        native.projection("/ui/arena/timeline")["pending"]["hash"],
        package.timeline.hash
    );
    // The selected runtime and seeding no longer depend on original package paths.
    std::fs::remove_dir_all(&directory.0).unwrap();
    native.start();
    native.tick();
    assert_eq!(
        native.projection("/ui/arena/content")["active"]["hash"],
        package.content.hash
    );
    assert_eq!(
        native.projection("/ui/arena/script")["active"]["hash"],
        package.script.hash
    );
    assert_eq!(
        native.projection("/ui/arena/timeline")["active"]["hash"],
        package.timeline.hash
    );
    drop(native);
    let baseline = Directory::package(false);
    // An edited authoring copy is diagnostic/next-run input, not startup authority.
    let restarted =
        Native::create(json!({"bundledContentDirectory":baseline.0,"storageDirectory":storage.0}))
            .unwrap();
    for path in &FILE_PATHS[1..] {
        assert_eq!(
            std::fs::read(storage.0.join(path)).unwrap(),
            package.source_bytes(path).unwrap()
        );
    }
    let defaults = load_package(&baseline.0).unwrap();
    restarted.start();
    restarted.tick();
    assert_eq!(
        restarted.projection("/ui/arena/content")["active"]["hash"],
        defaults.content.hash
    );
    assert_eq!(
        restarted.projection("/ui/arena/script")["active"]["hash"],
        defaults.script.hash
    );
    assert_eq!(
        restarted.projection("/ui/arena/timeline")["active"]["hash"],
        defaults.timeline.hash
    );
}

#[test]
fn explicit_authoring_sources_are_not_seeded_or_used_as_package_initializers() {
    let directory = Directory::package(true);
    let storage = Directory::new();
    let authoring = Directory::package(false);
    let original = FILE_PATHS[1..]
        .iter()
        .map(|path| std::fs::read(authoring.0.join(path)).unwrap())
        .collect::<Vec<_>>();
    let selected = load_package(&directory.0).unwrap();
    let native = Native::create(json!({"bundledContentDirectory":directory.0,"storageDirectory":storage.0,
        "contentPath":authoring.0.join("arena.tmd"),"scriptPath":authoring.0.join("boss.rhai"),"timelineDirectory":authoring.0.join("timelines")})).unwrap();
    for (path, bytes) in FILE_PATHS[1..].iter().zip(original) {
        assert_eq!(std::fs::read(authoring.0.join(path)).unwrap(), bytes);
        assert!(!storage.0.join(path).exists());
    }
    native.start();
    native.tick();
    assert_eq!(
        native.projection("/ui/arena/content")["active"]["hash"],
        selected.content.hash
    );
    assert_eq!(
        native.projection("/ui/arena/script")["active"]["hash"],
        selected.script.hash
    );
    assert_eq!(
        native.projection("/ui/arena/timeline")["active"]["hash"],
        selected.timeline.hash
    );
}

#[test]
fn invalid_or_mixed_package_initializers_fail_without_handle_storage_or_fallback() {
    let directory = Directory::package(false);
    let storage = Directory::new();
    let package = load_package(&directory.0).unwrap();
    for (field, value) in [
        ("content", serde_json::to_value(&package.content).unwrap()),
        ("script", serde_json::to_value(&package.script).unwrap()),
        ("timeline", serde_json::to_value(&package.timeline).unwrap()),
    ] {
        let mut config = json!({"bundledContentDirectory":directory.0,"storageDirectory":storage.0.join("uncreated")});
        config[field] = value;
        let error = Native::create(config)
            .err()
            .expect("mixed initializer refused");
        assert!(error.contains("cannot be combined"), "{error}");
        assert!(!storage.0.join("uncreated").exists());
    }
    assert!(
        Native::create(json!({"bundledContentDirectory":"relative"}))
            .err()
            .unwrap()
            .contains("absolute")
    );
    // Explicit nulls retain the optional initializer semantics.
    let native = Native::create(
        json!({"bundledContentDirectory":directory.0,"content":null,"script":null,"timeline":null}),
    )
    .unwrap();
    drop(native);
    for path in FILE_PATHS {
        let directory = Directory::package(false);
        std::fs::remove_file(directory.0.join(path)).unwrap();
        let error = Native::create(json!({"bundledContentDirectory":directory.0,"storageDirectory":storage.0.join("uncreated")})).err().unwrap();
        assert!(error.contains("invalid bundled Arena content"), "{error}");
        assert!(!storage.0.join("uncreated").exists());
    }
    let file = directory.0.join("boss.rhai");
    let mut source = std::fs::read(&file).unwrap();
    source[0] ^= 1;
    std::fs::write(file, source).unwrap();
    assert!(
        Native::create(json!({"bundledContentDirectory":directory.0}))
            .err()
            .unwrap()
            .contains("SHA256 mismatch")
    );
}

#[test]
fn expected_visual_package_identity_rejects_replacement_before_native_or_storage_creation() {
    let selected = Directory::package(false);
    let replacement = Directory::package(true);
    let storage = Directory::new();
    let expected = load_package(&selected.0).unwrap().identity.hash;
    let config = json!({"bundledContentDirectory":selected.0,"expectedBundledContentHash":expected,
        "storageDirectory":storage.0.join("uncreated")});
    // A coherent replacement is individually valid, but cannot be adopted with
    // already-loaded assets belonging to the previously verified manifest.
    for path in std::iter::once("package.json").chain(FILE_PATHS) {
        std::fs::copy(replacement.0.join(path), selected.0.join(path)).unwrap();
    }
    assert_ne!(load_package(&selected.0).unwrap().identity.hash, expected);
    let error = Native::create(config)
        .err()
        .expect("changed package refused");
    assert!(
        error.contains("changed after visual preparation"),
        "{error}"
    );
    assert!(!storage.0.join("uncreated").exists());
    for hash in ["a".repeat(63), "A".repeat(64), "g".repeat(64)] {
        let error = Native::create(
            json!({"bundledContentDirectory":selected.0,"expectedBundledContentHash":hash}),
        )
        .err()
        .unwrap();
        assert!(error.contains("64 lowercase hexadecimal"), "{error}");
    }
    assert!(
        Native::create(json!({"expectedBundledContentHash":expected}))
            .err()
            .unwrap()
            .contains("requires bundledContentDirectory")
    );
    let current = load_package(&selected.0).unwrap().identity.hash;
    let native = Native::create(
        json!({"bundledContentDirectory":selected.0,"expectedBundledContentHash":current}),
    )
    .unwrap();
    assert_eq!(native.metadata()["package"]["hash"], current);
}
