//! Ordered, detached source provenance and identity-based sparse patches.
use super::*;
use serde_json::Value;

/// Fixed override order, independent of JSON object or filesystem order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Layer {
    /// Complete initial configuration.
    Base,
    /// Difficulty adjustments.
    Difficulty,
    /// Event adjustments.
    Event,
    /// Local development overrides.
    Debug,
}

/// Authoring format; the evaluator is recorded separately.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SourceFormat {
    /// Public Tanu document/Formula API.
    Tmd,
    /// Native typed SQLite scalar snapshot.
    Sqlite,
}

/// Scalar-reading policy for Arena SQLite schema one; no Formula evaluation.
pub const SQLITE_EVALUATOR: &str = "arena-sqlite-scalars-v1";

/// Source identity retained without its authoring location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceDescriptor {
    /// Position in the fixed override order.
    pub layer: Layer,
    /// Actual storage format.
    pub format: SourceFormat,
    /// Arena source schema.
    pub schema_version: u32,
    /// Tanu revision or the SQLite scalar-reading policy.
    pub evaluator: String,
    /// Raw TMD digest or normalized queried SQLite snapshot digest.
    pub source_sha256: String,
}

/// Paths never enter this detached, bounded run/recording metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentProvenance {
    /// Provenance encoding version.
    pub version: u32,
    /// Declared sources in base, difficulty, event, debug order.
    pub sources: Vec<SourceDescriptor>,
    /// RFC6901-escaped semantic field paths and their winning layers.
    pub origins: BTreeMap<String, Layer>,
}

impl ContentProvenance {
    pub(super) fn digest(&self) -> Result<String> {
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }

    pub(super) fn validate(&self, values: &ArenaConfig) -> Result<()> {
        ensure!(self.version == 1, "unsupported content provenance version");
        ensure!(
            !self.sources.is_empty() && self.sources.len() <= 4,
            "source stack requires 1..4 sources"
        );
        ensure!(
            self.sources[0].layer == Layer::Base,
            "source stack must begin with base"
        );
        ensure!(
            self.sources
                .windows(2)
                .all(|pair| pair[0].layer < pair[1].layer),
            "source layers must be unique and ordered"
        );
        for source in &self.sources {
            ensure!(source.schema_version == 1, "unsupported source schema");
            let evaluator = match source.format {
                SourceFormat::Tmd => kitu_data_tmd::tables::TANU_REVISION,
                SourceFormat::Sqlite => SQLITE_EVALUATOR,
            };
            ensure!(
                source.evaluator == evaluator,
                "incompatible source evaluator"
            );
            ensure!(
                source.source_sha256.len() == 64
                    && source
                        .source_sha256
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit()),
                "invalid source snapshot digest"
            );
        }
        ensure!(
            self.origins.keys().collect::<Vec<_>>()
                == flattened(values)?.keys().collect::<Vec<_>>(),
            "content origins do not cover exactly the evaluated fields"
        );
        ensure!(
            self.origins
                .values()
                .all(|layer| self.sources.iter().any(|source| &source.layer == layer)),
            "content origin names an undeclared layer"
        );
        Ok(())
    }
}

/// Candidate change compared to a selected active or pending version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentDifference {
    /// Semantic field path (identity keys are RFC6901 escaped).
    pub path: String,
    /// Earlier value; null represents a previously absent base field.
    pub before: Value,
    /// Candidate value; null represents a field removed by a complete new base.
    pub after: Value,
    /// Layer supplying the candidate value (base for removed base identities).
    pub winning_layer: Layer,
}

/// Returns the winning layer for every semantic field, including legacy TMD.
///
/// # Errors
/// Rejects forged or incompatible detached content before inspecting origins.
pub fn content_origins(version: &ContentVersion) -> Result<BTreeMap<String, Layer>> {
    version.validate()?;
    Ok(version
        .provenance
        .as_ref()
        .map(|value| value.origins.clone())
        .unwrap_or(base_origins(&version.values)?))
}

/// Compares final values using stable identities rather than incidental row indices.
/// Chest order and repetitions are represented as one complete ordered-list change.
///
/// # Errors
/// Rejects invalid detached versions.
pub fn diff_content(
    before: &ContentVersion,
    after: &ContentVersion,
) -> Result<Vec<ContentDifference>> {
    before.validate()?;
    let origins = content_origins(after)?;
    let old = flattened(&before.values)?;
    let new = flattened(&after.values)?;
    let keys = old.keys().chain(new.keys()).collect::<BTreeSet<_>>();
    Ok(keys
        .into_iter()
        .filter_map(|path| {
            let before = old.get(path).cloned().unwrap_or(Value::Null);
            let after = new.get(path).cloned().unwrap_or(Value::Null);
            (before != after).then(|| ContentDifference {
                path: path.clone(),
                before,
                after,
                winning_layer: origins.get(path).copied().unwrap_or(Layer::Base),
            })
        })
        .collect())
}

fn escaped(id: &str) -> String {
    id.replace('~', "~0").replace('/', "~1")
}

fn flattened(values: &ArenaConfig) -> Result<BTreeMap<String, Value>> {
    let json = decimal_rows(values)?;
    let mut fields = BTreeMap::from([
        ("/schemaVersion".into(), json["schemaVersion"].clone()),
        ("/chests".into(), json["chests"].clone()),
    ]);
    for (table, identity) in [("items", "id"), ("enemies", "kind")] {
        for row in json[table].as_array().context("typed rows")? {
            let key = if identity == "id" {
                row[identity].as_str().context("typed id")?.to_owned()
            } else {
                row[identity].to_string()
            };
            for (column, value) in row.as_object().context("typed row")? {
                fields.insert(
                    format!("/{table}/{}/{column}", escaped(&key)),
                    value.clone(),
                );
            }
        }
    }
    for (column, value) in json["difficulty"].as_object().context("typed difficulty")? {
        fields.insert(format!("/difficulty/{column}"), value.clone());
    }
    Ok(fields)
}

pub(super) fn base_origins(values: &ArenaConfig) -> Result<BTreeMap<String, Layer>> {
    Ok(flattened(values)?
        .into_keys()
        .map(|key| (key, Layer::Base))
        .collect())
}

pub(super) fn columns(name: &str) -> Result<&'static [&'static str]> {
    match name {
        "items" => Ok(ITEM_COLUMNS),
        "enemies" => Ok(ENEMY_COLUMNS),
        "difficulty" => Ok(DIFFICULTY_COLUMNS),
        "chests" => Ok(CHEST_COLUMNS),
        _ => bail!("unknown Arena table {name}"),
    }
}

pub(super) fn tables_from_values(values: &ArenaConfig) -> Result<ArenaTables> {
    let json = decimal_rows(values)?;
    let mut tables = ArenaTables::new();
    for name in ["items", "enemies", "difficulty", "chests"] {
        let list = if name == "difficulty" {
            vec![json[name].clone()]
        } else {
            json[name].as_array().context("typed rows")?.clone()
        };
        let columns = columns(name)?;
        let rows = list
            .iter()
            .map(|row| {
                columns
                    .iter()
                    .map(|column| {
                        let value = &row[*column];
                        if let Some(text) = value.as_str() {
                            DataScalar::String(text.into())
                        } else if let Some(integer) = value.as_i64() {
                            DataScalar::Integer(integer)
                        } else {
                            DataScalar::Real(value.as_f64().expect("validated numeric content"))
                        }
                    })
                    .collect()
            })
            .collect();
        tables.insert(
            name.into(),
            DataTable {
                columns: columns.iter().map(|name| (*name).into()).collect(),
                rows,
            },
        );
    }
    Ok(tables)
}

fn typed_patch_field(table: &str, field: &str, value: &Value) -> Result<()> {
    let path = format!("{table}/{field}");
    if ["id", "name", "phase", "itemId"].contains(&field) {
        let text = value
            .as_str()
            .with_context(|| format!("{path}: expected text"))?;
        ensure!(
            !text.is_empty() && text.len() <= if field == "name" { 120 } else { 64 },
            "{path}: empty or oversized text"
        );
        if field == "phase" {
            ensure!(
                ["preparing", "boss"].contains(&text),
                "{path}: unknown chest phase"
            );
        }
    } else if ["kind", "damage", "bossDamage", "health"].contains(&field) {
        let number: i32 = serde_json::from_value(value.clone())
            .with_context(|| format!("{path}: expected integer"))?;
        let (min, max) = match (table, field) {
            ("items", "kind") => (0, 7),
            ("enemies", "kind") => (0, 3),
            ("enemies", "health") => (1, 1_000_000),
            ("enemies", "damage") => (1, 10_000),
            _ => (0, 10_000),
        };
        ensure!(
            (min..=max).contains(&number),
            "{path}: integer out of range"
        );
    } else {
        let value: f32 = serde_json::from_value(value.clone())
            .with_context(|| format!("{path}: expected number"))?;
        let (min, max) = match field {
            "healthGrowth" | "damageGrowth" => (0.0, 2.0),
            "radius" => (0.05, 3.0),
            "interval" if table == "enemies" => (0.05, 60.0),
            "interval" => (0.0, 60.0),
            _ => (0.0, 30.0),
        };
        number(value, min, max, &path)?;
    }
    Ok(())
}

pub(super) fn apply_patch(
    values: &ArenaConfig,
    patch: ArenaTables,
    layer: Layer,
    origins: &mut BTreeMap<String, Layer>,
) -> Result<ArenaConfig> {
    let mut complete = tables_from_values(values)?;
    let mut next_origins = origins.clone();
    for (name, table) in patch {
        let allowed = columns(&name)?;
        ensure!(
            table
                .columns
                .iter()
                .all(|column| allowed.contains(&column.as_str())),
            "{name}: unknown patch column"
        );
        ensure!(!table.columns.is_empty(), "{name}: patch requires a field");
        let maximum = if name == "chests" {
            128
        } else if name == "items" {
            64
        } else if name == "enemies" {
            4
        } else {
            1
        };
        ensure!(table.rows.len() <= maximum, "{name}: too many patch rows");
        let present = table.columns.iter().map(String::as_str).collect::<Vec<_>>();
        let patch_rows = rows::<Value>(table.clone(), &name, &present)?;
        for row in &patch_rows {
            for (field, value) in row.as_object().context("patch row")? {
                typed_patch_field(&name, field, value)?;
            }
        }
        if name == "chests" {
            ensure!(
                present.iter().copied().collect::<BTreeSet<_>>()
                    == CHEST_COLUMNS.iter().copied().collect(),
                "chests replacement requires every column"
            );
            complete.insert(name, table);
            next_origins.insert("/chests".into(), layer);
            continue;
        }
        if name == "difficulty" {
            ensure!(
                patch_rows.len() == 1,
                "difficulty patch requires exactly one row"
            );
        }
        let identity = match name.as_str() {
            "items" => Some("id"),
            "enemies" => Some("kind"),
            _ => None,
        };
        if let Some(key) = identity {
            ensure!(
                present.contains(&key),
                "{name}: patch requires identity {key}"
            );
        }
        let target = complete.get_mut(&name).expect("known complete table");
        let mut seen = BTreeSet::new();
        for (index, row) in patch_rows.iter().enumerate() {
            let key = identity.map(|field| row[field].clone());
            if let Some(key) = &key {
                ensure!(
                    seen.insert(key.to_string()),
                    "{name}: duplicate patch identity {key}"
                );
            }
            let target_index = if let Some(field) = identity {
                let column = target
                    .columns
                    .iter()
                    .position(|column| column == field)
                    .expect("identity column");
                target
                    .rows
                    .iter()
                    .position(|candidate| {
                        let value = match &candidate[column] {
                            DataScalar::String(value) => Value::String(value.clone()),
                            DataScalar::Integer(value) => (*value).into(),
                            _ => Value::Null,
                        };
                        Some(value) == key
                    })
                    .with_context(|| {
                        format!("{name}: patch cannot add unknown identity {}", row[field])
                    })?
            } else {
                0
            };
            for (column, field) in table.columns.iter().enumerate() {
                if Some(field.as_str()) == identity {
                    continue;
                }
                let destination = target
                    .columns
                    .iter()
                    .position(|name| name == field)
                    .expect("allowed column");
                target.rows[target_index][destination] = table.rows[index][column].clone();
                let path = if let Some(identity) = identity {
                    let key = if identity == "id" {
                        row[identity].as_str().expect("typed id").to_owned()
                    } else {
                        row[identity].to_string()
                    };
                    format!("/{name}/{}/{field}", escaped(&key))
                } else {
                    format!("/{name}/{field}")
                };
                next_origins.insert(path, layer);
            }
        }
    }
    let merged = ArenaConfig::from_tables(complete)?;
    *origins = next_origins;
    Ok(merged)
}
