use sha2::{Digest, Sha256};
use std::{
    env, fs,
    path::{Path, PathBuf},
};

fn files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        println!("cargo:rerun-if-changed={}", path.display());
        for entry in fs::read_dir(path).unwrap() {
            files(&entry.unwrap().path(), out);
        }
    } else if path.extension().is_some_and(|ext| ext == "rs") {
        out.push(path.to_path_buf());
    }
}
fn main() {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let mut paths = vec![
        root.join("Cargo.lock"),
        root.join("apps/demo-game/Cargo.toml"),
        root.join("apps/demo-game/content/arena-default.json"),
        root.join("apps/demo-game/content/boss.rhai"),
        root.join("crates/kitu-scripting-rhai/Cargo.toml"),
        root.join("apps/demo-game/src/arena.rs"),
    ];
    files(&root.join("apps/demo-game/src/arena"), &mut paths);
    for name in [
        "kitu-runtime",
        "kitu-core",
        "kitu-ecs",
        "kitu-osc-ir",
        "kitu-transport",
        "kitu-scripting-rhai",
    ] {
        files(&root.join("crates").join(name).join("src"), &mut paths);
    }
    paths.sort();
    let mut hash = Sha256::new();
    let compiler = std::process::Command::new(env::var("RUSTC").unwrap())
        .arg("--version")
        .output()
        .expect("read Rust compiler version");
    assert!(compiler.status.success());
    hash.update(compiler.stdout);
    for key in ["OPT_LEVEL", "DEBUG", "CARGO_ENCODED_RUSTFLAGS"] {
        println!("cargo:rerun-if-env-changed={key}");
        hash.update(key.as_bytes());
        hash.update(env::var(key).unwrap_or_default().as_bytes());
    }
    for path in paths {
        println!("cargo:rerun-if-changed={}", path.display());
        hash.update(
            path.strip_prefix(&root)
                .unwrap()
                .to_str()
                .unwrap()
                .as_bytes(),
        );
        hash.update([0]);
        hash.update(fs::read(path).unwrap());
        hash.update([0]);
    }
    println!(
        "cargo:rustc-env=ARENA_EXECUTION_HASH={}",
        hex::encode(hash.finalize())
    );
    println!(
        "cargo:rustc-env=ARENA_EXECUTION_TARGET={}",
        env::var("TARGET").unwrap()
    );
}
