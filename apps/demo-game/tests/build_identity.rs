#[allow(dead_code)]
#[path = "../build_identity.rs"]
mod build_identity;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "kitu-identity-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        for name in ["app", "kitu"] {
            fs::create_dir_all(root.join(name).join("src")).unwrap();
            fs::write(root.join(name).join("src/lib.rs"), "pub fn run() {}\n").unwrap();
            fs::write(
                root.join(name).join("Cargo.toml"),
                format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n"),
            )
            .unwrap();
        }
        fs::write(root.join("Cargo.lock"), "version = 4\n").unwrap();
        let fixture = Self {
            root: root.canonicalize().unwrap(),
        };
        let mut roots = vec![];
        let mut guards = vec![];
        for name in ["app", "kitu"] {
            let path = fixture.root.join(name);
            let sha = hex::encode(Sha256::digest(fs::read(path.join("Cargo.toml")).unwrap()));
            roots.push(json!({"logical": format!("{name}|0.1.0|path"), "path": path, "manifest_sha256": sha,
                              "inputs": ["Cargo.toml", "src"]}));
            guards.push(json!({"path": path.join("Cargo.toml"), "sha256": sha}));
        }
        guards.push(json!({"path": fixture.root.join("Cargo.lock"), "sha256": hex::encode(Sha256::digest(b"version = 4\n"))}));
        fixture.save(&json!({"schema": 1, "app_root": fixture.root.join("app"), "target": "test-target",
            "features": [], "roots": roots, "guards": guards, "build_configuration": {},
            "graph": [{"package": "app|0.1.0|path", "features": [], "dependencies": [{"package": "kitu|0.1.0|path"}]},
                      {"package": "kitu|0.1.0|path", "features": [], "dependencies": []}]}));
        fixture
    }
    fn load(&self) -> Value {
        serde_json::from_slice(&fs::read(self.root.join("app/kitu-build-inputs.json")).unwrap())
            .unwrap()
    }
    fn save(&self, value: &Value) {
        fs::write(
            self.root.join("app/kitu-build-inputs.json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    fn hash(&self) -> String {
        self.hash_with("test-target", &BTreeSet::new(), &[])
    }
    fn hash_with(
        &self,
        target: &str,
        features: &BTreeSet<String>,
        settings: &[(String, String)],
    ) -> String {
        build_identity::identity(
            &self.root.join("app"),
            target,
            features,
            settings,
            b"rustc test",
        )
    }
    fn rejects(&self) {
        assert!(std::panic::catch_unwind(|| self.hash()).is_err());
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn relocating_inputs_preserves_identity() {
    let a = Fixture::new();
    let b = Fixture::new();
    assert_ne!(a.root, b.root);
    assert_eq!(a.hash(), b.hash());
}
#[test]
fn edits_at_same_revision_and_new_source_files_change_identity() {
    let fixture = Fixture::new();
    let original = fixture.hash();
    let source = fixture.root.join("kitu/src/lib.rs");
    fs::write(&source, "pub fn run() { panic!(); }\n").unwrap();
    assert_ne!(original, fixture.hash());
    fs::write(&source, "pub fn run() {}\n").unwrap();
    assert_eq!(original, fixture.hash());
    fs::write(
        fixture.root.join("kitu/src/new.rs"),
        "pub const NEW: u8 = 1;\n",
    )
    .unwrap();
    assert_ne!(original, fixture.hash());
}
#[test]
fn graph_profile_and_flags_change_identity() {
    let fixture = Fixture::new();
    let original = fixture.hash();
    assert_ne!(
        original,
        fixture.hash_with(
            "test-target",
            &BTreeSet::new(),
            &[("OPT_LEVEL".into(), "3".into())]
        )
    );
    let mut map = fixture.load();
    map["graph"][1]["features"] = json!(["patched-feature"]);
    fixture.save(&map);
    assert_ne!(original, fixture.hash());
    let before_profile = fixture.hash();
    map["build_configuration"] = json!({"profiles": {"release": {"overflow-checks": true}}});
    fixture.save(&map);
    assert_ne!(before_profile, fixture.hash());
}
#[test]
fn rejects_stale_lockfile_and_manifest() {
    let fixture = Fixture::new();
    fs::write(fixture.root.join("Cargo.lock"), "version = 3\n").unwrap();
    fixture.rejects();
    fs::write(fixture.root.join("Cargo.lock"), "version = 4\n").unwrap();
    fs::write(
        fixture.root.join("kitu/Cargo.toml"),
        "[package]\nname = \"different\"\n",
    )
    .unwrap();
    fixture.rejects();
}
#[test]
fn rejects_missing_roots_and_target_or_feature_mismatch() {
    let fixture = Fixture::new();
    assert!(
        std::panic::catch_unwind(|| fixture.hash_with("wrong-target", &BTreeSet::new(), &[]))
            .is_err()
    );
    assert!(std::panic::catch_unwind(|| fixture.hash_with(
        "test-target",
        &BTreeSet::from(["OTHER".into()]),
        &[]
    ))
    .is_err());
    let mut map = fixture.load();
    map["roots"].as_array_mut().unwrap().pop();
    fixture.save(&map);
    fixture.rejects();
}
#[test]
fn rejects_wrong_app_and_escaping_source_input() {
    let fixture = Fixture::new();
    let mut map = fixture.load();
    map["app_root"] = json!(fixture.root.join("kitu"));
    fixture.save(&map);
    fixture.rejects();
    map["app_root"] = json!(fixture.root.join("app"));
    map["roots"][1]["inputs"]
        .as_array_mut()
        .unwrap()
        .push(json!("../app/src"));
    fixture.save(&map);
    fixture.rejects();
}
