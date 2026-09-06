//! Verified, relocatable source packages for native Arena initialization.
//!
//! Files are captured once, hashed, then evaluated with the existing application
//! loaders before tick zero. Package identity is host metadata, never game state.
use super::{config::ContentVersion, presentation::TimelineVersion, script::ScriptVersion};
use anyhow::{ensure, Context, Result};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs::OpenOptions, io::Read, path::Path};

/// Maximum exact manifest byte length, including whitespace.
pub const MAX_MANIFEST_BYTES: usize = 16 * 1024;
/// Complete ordered source allowlist. Manifest entries cannot add paths.
pub const FILE_PATHS: [&str; 5] = [
    "unity-assets.json",
    "arena.tmd",
    "boss.rhai",
    "timelines/boss-telegraph.tsq",
    "timelines/floor-transition.tsq",
];
/// Inclusive source byte limits in [`FILE_PATHS`] order.
pub const MAX_FILE_BYTES: [usize; 5] = [8 * 1024, 128 * 1024, 64 * 1024, 8 * 1024, 8 * 1024];
/// Default role/key/type mapping included in authored packages.
pub const DEFAULT_UNITY_ASSETS: &[u8] = include_bytes!("../../content/unity-assets.json");

/// Fixed-path payload description; hashes cover the original file bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageFile {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}
/// Manifest schema1. Identity is the hash of its exact on-disk JSON bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageManifest {
    pub schema_version: u32,
    pub files: Vec<PackageFile>,
}
/// One named visual role and the Addressables key/type requested by the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnityAsset {
    pub role: String,
    pub key: String,
    #[serde(rename = "type")]
    pub asset_type: String,
}
/// The four exact roles required by the existing Arena view.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnityAssets {
    pub schema_version: u32,
    pub assets: Vec<UnityAsset>,
}
/// Host-only package and evaluated source identities, independent of location.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PackageIdentity {
    pub schema_version: u32,
    pub hash: String,
    pub files: Vec<PackageFile>,
    pub content_hash: String,
    pub script_hash: String,
    pub timeline_hash: String,
}
/// Detached validated initial versions plus exact bytes available for authoring seeding.
#[derive(Debug, Clone)]
pub struct LoadedPackage {
    pub identity: PackageIdentity,
    pub content: ContentVersion,
    pub script: ScriptVersion,
    pub timeline: TimelineVersion,
    pub assets: UnityAssets,
    sources: Vec<Vec<u8>>,
}
impl LoadedPackage {
    /// Returns the captured bytes for an allowlisted source; never rereads disk.
    pub fn source_bytes(&self, path: &str) -> Option<&[u8]> {
        FILE_PATHS
            .iter()
            .position(|candidate| *candidate == path)
            .map(|index| self.sources[index].as_slice())
    }
}
/// Validates and evaluates a complete explicit native initialization package.
///
/// Rejects missing, modified, non-regular and unsupported source files rather
/// than replacing them with compiled defaults. No Runtime is created or ticked.
///
/// ```no_run
/// use std::path::Path;
/// use kitu_demo_game::arena::package::load_package;
/// let package = load_package(Path::new("/Applications/Arena.app/Contents/Resources/Data/StreamingAssets/KituArena"))?;
/// assert_eq!(package.identity.files.len(), 5);
/// assert!(package.source_bytes("arena.tmd").is_some());
/// # Ok::<(), anyhow::Error>(())
/// ```
pub fn load_package(directory: &Path) -> Result<LoadedPackage> {
    ensure!(
        directory.is_absolute(),
        "bundledContentDirectory must be an absolute path"
    );
    ensure!(
        std::fs::symlink_metadata(directory)?.file_type().is_dir(),
        "bundledContentDirectory must be a real directory"
    );
    let manifest_bytes = read_regular(&directory.join("package.json"), MAX_MANIFEST_BYTES)?;
    let manifest: PackageManifest =
        named_json(&manifest_bytes, "files").context("invalid package manifest")?;
    ensure!(
        manifest.schema_version == 1,
        "package schemaVersion must be 1"
    );
    ensure!(
        manifest.files.len() == FILE_PATHS.len(),
        "package requires exactly five source files"
    );
    for ((file, path), maximum) in manifest.files.iter().zip(FILE_PATHS).zip(MAX_FILE_BYTES) {
        ensure!(
            file.path == path,
            "package source paths/order must match the fixed allowlist"
        );
        ensure!(
            file.bytes <= maximum as u64,
            "package source {path} exceeds byte limit"
        );
        ensure!(
            file.sha256.len() == 64
                && file
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
            "package source {path} requires a lowercase SHA256 digest"
        );
    }
    ensure!(
        std::fs::symlink_metadata(directory.join("timelines"))?
            .file_type()
            .is_dir(),
        "package timelines must be a real directory"
    );
    let mut sources = Vec::with_capacity(FILE_PATHS.len());
    for (file, maximum) in manifest.files.iter().zip(MAX_FILE_BYTES) {
        let bytes = read_regular(&directory.join(&file.path), maximum)?;
        ensure!(
            bytes.len() as u64 == file.bytes,
            "package source {} byte length mismatch",
            file.path
        );
        ensure!(
            hash(&bytes) == file.sha256,
            "package source {} SHA256 mismatch",
            file.path
        );
        sources.push(bytes);
    }
    let assets: UnityAssets =
        named_json(&sources[0], "assets").context("invalid Unity asset mapping")?;
    validate_assets(&assets)?;
    let content = ContentVersion::from_tmd(&sources[1]).context("invalid packaged TMD")?;
    let source = std::str::from_utf8(&sources[2]).context("packaged Rhai must be UTF-8")?;
    let script = ScriptVersion::from_source(source).context("invalid packaged Rhai")?;
    let timeline =
        TimelineVersion::from_sources(&sources[3], &sources[4]).context("invalid packaged TSQ1")?;
    let identity = PackageIdentity {
        schema_version: 1,
        hash: hash(&manifest_bytes),
        files: manifest.files,
        content_hash: content.hash.clone(),
        script_hash: script.hash.clone(),
        timeline_hash: timeline.hash.clone(),
    };
    Ok(LoadedPackage {
        identity,
        content,
        script,
        timeline,
        assets,
        sources,
    })
}
fn validate_assets(mapping: &UnityAssets) -> Result<()> {
    ensure!(
        mapping.schema_version == 1,
        "Unity asset schemaVersion must be 1"
    );
    ensure!(
        mapping.assets.len() == 4,
        "Unity mapping requires exactly four roles"
    );
    let mut keys = BTreeSet::new();
    for (entry, (role, asset_type)) in mapping.assets.iter().zip([
        ("baseMaterial", "Material"),
        ("cube", "GameObject"),
        ("capsule", "GameObject"),
        ("sphere", "GameObject"),
    ]) {
        ensure!(
            entry.role == role && entry.asset_type == asset_type,
            "Unity mapping roles/order/types must match the Arena contract"
        );
        ensure!(
            !entry.key.is_empty() && entry.key.len() <= 128 && !entry.key.contains('\0'),
            "Unity asset key must be 1..128 UTF-8 bytes without NUL"
        );
        ensure!(keys.insert(&entry.key), "Unity asset keys must be unique");
    }
    Ok(())
}
fn named_json<T: DeserializeOwned>(bytes: &[u8], list: &str) -> Result<T> {
    // These documents are at most16KiB. Check named-map shapes before the typed
    // deserializer, which independently rejects duplicate/unknown fields.
    let value: serde_json::Value = serde_json::from_slice(bytes)?;
    ensure!(value.is_object(), "JSON document must be a named object");
    let entries = value
        .get(list)
        .and_then(serde_json::Value::as_array)
        .context("required source/asset list is missing or has the wrong type")?;
    ensure!(
        entries.iter().all(serde_json::Value::is_object),
        "source/asset entries must be named objects"
    );
    Ok(serde_json::from_slice(bytes)?)
}
fn read_regular(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    ensure!(
        std::fs::symlink_metadata(path)?.file_type().is_file(),
        "package source must be a regular file: {}",
        path.display()
    );
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW);
    }
    let file = options
        .open(path)
        .with_context(|| format!("open package source {}", path.display()))?;
    let metadata = file.metadata()?;
    ensure!(metadata.is_file(), "package source must be a regular file");
    ensure!(
        metadata.len() <= maximum as u64,
        "package source exceeds byte limit"
    );
    let mut bytes = Vec::new();
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() <= maximum, "package source exceeds byte limit");
    Ok(bytes)
}
fn hash(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests;
