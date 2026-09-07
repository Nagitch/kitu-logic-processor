//! App-owned source schemas and authoring; storage paths stay outside run data.
use super::*;
use kitu_data_sqlite::{ReadOptions, Scalar, TableSpec};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

/// A source location used only for authoring diagnostics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceLocation {
    /// Fixed stack slot.
    pub layer: Layer,
    /// Authoring format.
    pub format: SourceFormat,
    /// Resolved source file, excluded from detached provenance and hashes.
    pub path: PathBuf,
}

/// One declared file in a versioned source plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceReference {
    /// Explicit reader selection; no content sniffing or client-supplied SQL.
    pub format: SourceFormat,
    /// Absolute path or path relative to the source plan's directory.
    pub path: PathBuf,
}

/// Version one always resolves base → difficulty → event → debug.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourcePlan {
    /// Plan schema, currently one.
    pub version: u32,
    /// Complete initial configuration.
    pub base: SourceReference,
    /// Optional sparse difficulty override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub difficulty: Option<SourceReference>,
    /// Optional sparse event override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<SourceReference>,
    /// Optional sparse local override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debug: Option<SourceReference>,
}

/// Validated detached candidate and its separate diagnostic file locations.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadedContent {
    /// Safe to stage after the caller reviews its value and source hashes.
    pub version: ContentVersion,
    /// Source paths for host/UI diagnostics only.
    pub sources: Vec<SourceLocation>,
}

/// Reads a `.tmd`, `.sqlite`, or versioned `.arena.json` source plan.
///
/// # Errors
/// Returns source-specific I/O, evaluator, schema, type, identity or size errors.
pub fn load_content(path: &Path) -> Result<LoadedContent> {
    load_content_with_cancel(path, || false)
}

/// Reads outside the game tick, with cancellation between sources/tables and
/// inside SQLite execution. Successful candidates own all values and metadata.
/// Each public Tanu read/evaluate call completes before cancellation is observed;
/// Tanu's eager document validation is preserved and is not interruptible here.
///
/// # Errors
/// Invalid/cancelled authoring never produces a partially validated candidate.
pub fn load_content_with_cancel(
    path: &Path,
    cancelled: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<LoadedContent> {
    ensure!(
        !path.as_os_str().is_empty(),
        "content source is disabled for this host"
    );
    let cancelled: Arc<dyn Fn() -> bool + Send + Sync> = Arc::new(cancelled);
    check_cancel(&cancelled)?;
    let absolute = std::path::absolute(path).context("resolve content source")?;
    let mut locations = Vec::new();
    let legacy = absolute
        .extension()
        .is_some_and(|extension| extension == "tmd");
    if absolute.to_string_lossy().ends_with(".arena.json") {
        let plan: SourcePlan = serde_json::from_slice(&read_bounded(&absolute, MAX_CONTENT_BYTES)?)
            .with_context(|| format!("read source plan {}", absolute.display()))?;
        ensure!(plan.version == 1, "unsupported Arena source plan version");
        let parent = absolute.parent().context("source plan directory")?;
        for (layer, reference) in [
            (Layer::Base, Some(plan.base)),
            (Layer::Difficulty, plan.difficulty),
            (Layer::Event, plan.event),
            (Layer::Debug, plan.debug),
        ] {
            if let Some(reference) = reference {
                ensure!(
                    !reference.path.as_os_str().is_empty(),
                    "{layer:?}: empty source path"
                );
                locations.push(SourceLocation {
                    layer,
                    format: reference.format,
                    path: parent.join(reference.path),
                });
            }
        }
    } else {
        let format = match absolute.extension().and_then(|value| value.to_str()) {
            Some("tmd") => SourceFormat::Tmd,
            Some("sqlite") => SourceFormat::Sqlite,
            _ => bail!("expected .tmd, .sqlite or .arena.json content source"),
        };
        locations.push(SourceLocation {
            layer: Layer::Base,
            format,
            path: absolute,
        });
    }
    // A single legacy TMD keeps precisely the existing detached JSON encoding.
    if legacy {
        let bytes = read_bounded(&locations[0].path, 16 * 1024 * 1024)?;
        check_cancel(&cancelled)?;
        let tables = tmd_tables_checked(&bytes, || check_cancel(&cancelled))
            .with_context(|| format!("base source {}", locations[0].path.display()))?;
        let values = ArenaConfig::from_tables(tables)
            .with_context(|| format!("base source {}", locations[0].path.display()))?;
        let version = ContentVersion::from_evaluated_tmd(&bytes, values)
            .with_context(|| format!("base source {}", locations[0].path.display()))?;
        version.validate()?;
        check_cancel(&cancelled)?;
        return Ok(LoadedContent {
            version,
            sources: locations,
        });
    }
    let mut values = None;
    let mut provenance = ContentProvenance {
        version: 1,
        sources: Vec::new(),
        origins: BTreeMap::new(),
    };
    for location in &locations {
        check_cancel(&cancelled)?;
        let (tables, digest) = read_source(location, Arc::clone(&cancelled))
            .with_context(|| format!("{:?} source {}", location.layer, location.path.display()))?;
        let next = if let Some(previous) = &values {
            layers::apply_patch(previous, tables, location.layer, &mut provenance.origins)
                .with_context(|| {
                    format!("{:?} source {}", location.layer, location.path.display())
                })?
        } else {
            let base = ArenaConfig::from_tables(tables)
                .with_context(|| format!("base source {}", location.path.display()))?;
            provenance.origins = layers::base_origins(&base)?;
            base
        };
        values = Some(next);
        provenance.sources.push(SourceDescriptor {
            layer: location.layer,
            format: location.format,
            schema_version: 1,
            evaluator: match location.format {
                SourceFormat::Tmd => kitu_data_tmd::tables::TANU_REVISION,
                SourceFormat::Sqlite => layers::SQLITE_EVALUATOR,
            }
            .into(),
            source_sha256: digest,
        });
    }
    check_cancel(&cancelled)?;
    let values = values.context("source plan requires base")?;
    let version = ContentVersion {
        hash: values.hash()?,
        source_sha256: provenance.digest()?,
        tanu_revision: None,
        provenance: Some(provenance),
        values,
    };
    version.validate()?;
    Ok(LoadedContent {
        version,
        sources: locations,
    })
}

fn check_cancel(cancelled: &Arc<dyn Fn() -> bool + Send + Sync>) -> Result<()> {
    ensure!(!cancelled(), "content loading cancelled");
    Ok(())
}

fn read_bounded(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)
        .with_context(|| format!("open {}", path.display()))?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= maximum,
        "{} exceeds source size limit",
        path.display()
    );
    Ok(bytes)
}

pub(super) fn tmd_tables(bytes: &[u8]) -> Result<ArenaTables> {
    tmd_tables_checked(bytes, || Ok(()))
}

fn tmd_tables_checked(bytes: &[u8], check: impl Fn() -> Result<()>) -> Result<ArenaTables> {
    check()?;
    let document = TanuDocument::read(bytes).context("read Tanu document")?;
    document
        .source_names()?
        .into_iter()
        .map(|name| {
            check()?;
            layers::columns(&name)?;
            let table = document
                .table(&name)
                .with_context(|| format!("evaluate Tanu table {name}"))?;
            check()?;
            Ok((name, table))
        })
        .collect()
}

fn read_source(
    location: &SourceLocation,
    cancelled: Arc<dyn Fn() -> bool + Send + Sync>,
) -> Result<(ArenaTables, String)> {
    match location.format {
        SourceFormat::Tmd => {
            let bytes = read_bounded(&location.path, 16 * 1024 * 1024)?;
            check_cancel(&cancelled)?;
            let tables = tmd_tables_checked(&bytes, || check_cancel(&cancelled))?;
            check_cancel(&cancelled)?;
            Ok((tables, hex::encode(Sha256::digest(bytes))))
        }
        SourceFormat::Sqlite => {
            let specs = ["chests", "difficulty", "enemies", "items"]
                .into_iter()
                .map(|name| {
                    let columns = layers::columns(name)
                        .expect("Arena table")
                        .iter()
                        .map(|value| (*value).to_owned())
                        .collect::<Vec<_>>();
                    let required_columns = if location.layer == Layer::Base || name == "chests" {
                        columns.clone()
                    } else {
                        match name {
                            "items" => vec!["id".into()],
                            "enemies" => vec!["kind".into()],
                            _ => vec![],
                        }
                    };
                    TableSpec {
                        name: name.into(),
                        columns,
                        required_columns,
                        order_by: vec!["ordinal".into()],
                        optional: location.layer != Layer::Base,
                    }
                })
                .collect::<Vec<_>>();
            let options = ReadOptions {
                expected_schema_version: Some(1),
                deny_unknown_tables: true,
                max_tables: 4,
                max_columns: 8,
                max_rows_per_table: 128,
                max_total_cells: 4096,
                max_value_bytes: 1024,
                max_total_bytes: MAX_CONTENT_BYTES,
                max_vm_steps: 1_000_000,
                ..ReadOptions::default()
            };
            let snapshot =
                kitu_data_sqlite::read_snapshot(&location.path, &specs, &options, move || {
                    cancelled()
                })?;
            let digest = hex::encode(Sha256::digest(serde_json::to_vec(&snapshot)?));
            let tables = snapshot
                .tables
                .into_iter()
                .map(|table| {
                    let rows = table
                        .rows
                        .into_iter()
                        .map(|row| {
                            row.into_iter()
                                .map(|value| match value {
                                    Scalar::Null => DataScalar::Null,
                                    Scalar::Integer(value) => DataScalar::Integer(value),
                                    Scalar::Real(value) => DataScalar::Real(value),
                                    Scalar::Text(value) => DataScalar::String(value),
                                })
                                .collect()
                        })
                        .collect();
                    (
                        table.name,
                        DataTable {
                            columns: table.columns,
                            rows,
                        },
                    )
                })
                .collect();
            Ok((tables, digest))
        }
    }
}

/// Creates an Arena SQLite authoring database, including sparse patch tables.
/// Rows use unique integer ordinals, preserving order and duplicate chest entries.
///
/// # Errors
/// Rejects existing files, unknown/duplicate columns and malformed row widths.
/// Full game constraints are checked by `load_content` before staging.
pub fn write_sqlite(path: &Path, tables: &ArenaTables) -> Result<()> {
    for (name, table) in tables {
        let allowed = layers::columns(name)?;
        ensure!(
            !table.columns.is_empty()
                && table
                    .columns
                    .iter()
                    .all(|name| allowed.contains(&name.as_str())),
            "{name}: unknown or empty authoring columns"
        );
        ensure!(
            table.columns.iter().collect::<BTreeSet<_>>().len() == table.columns.len(),
            "{name}: duplicate authoring columns"
        );
        ensure!(
            table
                .rows
                .iter()
                .all(|row| row.len() == table.columns.len()),
            "{name}: authoring row width mismatch"
        );
        for value in table.rows.iter().flatten() {
            match value {
                DataScalar::Boolean(_) => bail!("Arena has no Boolean authoring fields"),
                DataScalar::Real(value) => ensure!(value.is_finite(), "non-finite authoring value"),
                _ => {}
            }
        }
    }
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create SQLite {}", path.display()))?;
    let result = populate_sqlite(path, tables);
    if result.is_err() {
        // Only the file this invocation reserved with create_new is removed;
        // an existing author's database never reaches this branch.
        let _ = std::fs::remove_file(path);
    }
    result
}

fn populate_sqlite(path: &Path, tables: &ArenaTables) -> Result<()> {
    let mut database = rusqlite::Connection::open(path)?;
    let transaction = database.transaction()?;
    transaction.pragma_update(None, "user_version", 1)?;
    for (name, table) in tables {
        // Only fixed app-schema names reach SQL. Untyped data columns preserve
        // their supplied native scalar type rather than applying text coercion.
        let columns = table
            .columns
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>();
        transaction.execute_batch(&format!(
            "CREATE TABLE \"{name}\" (ordinal INTEGER PRIMARY KEY, {})",
            columns.join(", ")
        ))?;
        let placeholders = std::iter::repeat_n("?", columns.len() + 1)
            .collect::<Vec<_>>()
            .join(",");
        let mut insert = transaction.prepare(&format!(
            "INSERT INTO \"{name}\" (ordinal,{}) VALUES ({placeholders})",
            columns.join(",")
        ))?;
        for (ordinal, row) in table.rows.iter().enumerate() {
            let mut parameters = vec![rusqlite::types::Value::Integer(i64::try_from(ordinal)?)];
            for value in row {
                parameters.push(match value {
                    DataScalar::Null => rusqlite::types::Value::Null,
                    DataScalar::Integer(value) => rusqlite::types::Value::Integer(*value),
                    DataScalar::Real(value) => {
                        ensure!(value.is_finite(), "non-finite authoring value");
                        rusqlite::types::Value::Real(*value)
                    }
                    DataScalar::String(value) => rusqlite::types::Value::Text(value.clone()),
                    DataScalar::Boolean(_) => bail!("Arena has no Boolean authoring fields"),
                });
            }
            insert.execute(rusqlite::params_from_iter(parameters))?;
        }
    }
    transaction.commit()?;
    Ok(())
}

fn write_new(path: &Path, bytes: &[u8]) -> Result<()> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?
        .write_all(bytes)?;
    Ok(())
}

fn write_patch_tmd(path: &Path, tables: &ArenaTables) -> Result<()> {
    let mut sources = BTreeMap::new();
    for (name, table) in tables {
        let columns = table.columns.iter().map(String::as_str).collect::<Vec<_>>();
        let values = rows::<serde_json::Value>(table.clone(), name, &columns)?;
        sources.insert(
            name.clone(),
            source(serde_json::to_value(values)?, &columns)?,
        );
    }
    write_new(path, &TanuDocument::create("# Arena sparse override\n\nEdit these managed tables; validate the source plan in Admin.\n".into(), sources)?.bytes()?)
}

/// Creates a reproducible editable four-layer example beside `PATH.arena.json`.
/// Existing files are not overwritten; the complete SQLite base preserves defaults.
///
/// # Errors
/// Rejects unsupported output names, existing files and authoring I/O errors.
pub fn create_source_plan(path: &Path) -> Result<()> {
    ensure!(
        path.to_string_lossy().ends_with(".arena.json"),
        "source plan must end in .arena.json"
    );
    let parent = path
        .parent()
        .filter(|value| !value.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let prefix = path
        .file_stem()
        .context("source plan filename")?
        .to_string_lossy();
    let reference = |suffix: &str, format| SourceReference {
        format,
        path: format!("{prefix}-{suffix}").into(),
    };
    let plan = SourcePlan {
        version: 1,
        base: reference("base.sqlite", SourceFormat::Sqlite),
        difficulty: Some(reference("difficulty.tmd", SourceFormat::Tmd)),
        event: Some(reference("event.sqlite", SourceFormat::Sqlite)),
        debug: Some(reference("debug.tmd", SourceFormat::Tmd)),
    };
    for reference in [
        &plan.base,
        plan.difficulty.as_ref().unwrap(),
        plan.event.as_ref().unwrap(),
        plan.debug.as_ref().unwrap(),
    ] {
        ensure!(
            !parent.join(&reference.path).exists(),
            "authoring source already exists: {}",
            reference.path.display()
        );
    }
    ensure!(!path.exists(), "source plan already exists");
    write_sqlite(
        &parent.join(&plan.base.path),
        &ArenaConfig::default().to_tables()?,
    )?;
    let item_patch = |damage| {
        BTreeMap::from([(
            "items".into(),
            DataTable {
                columns: vec!["id".into(), "damage".into()],
                rows: vec![vec![
                    DataScalar::String("starter".into()),
                    DataScalar::Integer(damage),
                ]],
            },
        )])
    };
    write_patch_tmd(
        &parent.join(&plan.difficulty.as_ref().unwrap().path),
        &item_patch(24),
    )?;
    write_sqlite(
        &parent.join(&plan.event.as_ref().unwrap().path),
        &item_patch(28),
    )?;
    write_patch_tmd(
        &parent.join(&plan.debug.as_ref().unwrap().path),
        &item_patch(32),
    )?;
    write_new(path, &serde_json::to_vec_pretty(&plan)?)?;
    Ok(())
}
