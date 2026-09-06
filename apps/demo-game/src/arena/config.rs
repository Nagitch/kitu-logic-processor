//! Typed Arena content. Tanu owns containers and Formula; this module owns game
//! constraints, immutable evaluated values and their semantic content hash.

use anyhow::{bail, ensure, Context, Result};
use kitu_data_tmd::tables::{
    DataScalar, DataSourceDefinition, DataTable, FormulaCellConstraint, FormulaTableCell,
    FormulaTableCellContent, FormulaTableColumn, FormulaTableLiteral, FormulaTableRow,
    TanuDocument,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

mod layers;
mod sources;
pub use layers::{
    content_origins, diff_content, ContentDifference, ContentProvenance, Layer, SourceDescriptor,
    SourceFormat,
};
pub use sources::{
    create_source_plan, load_content, load_content_with_cancel, write_sqlite, LoadedContent,
    SourceLocation, SourcePlan, SourceReference,
};

/// Largest detached content request accepted by Arena's ordinary input queue.
pub const MAX_CONTENT_BYTES: usize = 128 * 1024;

/// Evaluated tables shared by the real Tanu and SQLite adapters.
pub type ArenaTables = BTreeMap<String, DataTable>;

/// Evaluated values and provenance retained with a run; the file may change later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContentVersion {
    /// SHA-256 of the canonical evaluated values.
    pub hash: String,
    /// SHA-256 of legacy TMD bytes, or the complete ordered source provenance.
    pub source_sha256: String,
    /// Tanu API revision that evaluated the source.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tanu_revision: Option<String>,
    /// Detached source kinds, digests and field origins; never filesystem paths.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<ContentProvenance>,
    /// Complete values needed to reproduce gameplay without re-evaluating a file.
    pub values: ArenaConfig,
}

impl ContentVersion {
    /// Evaluates and validates an entire TMD candidate before Runtime admission.
    ///
    /// # Errors
    /// Returns the document, Formula or typed game validation diagnostic.
    pub fn from_tmd(bytes: &[u8]) -> Result<Self> {
        let values = ArenaConfig::from_tmd(bytes)?;
        Self::from_evaluated_tmd(bytes, values)
    }

    fn from_evaluated_tmd(bytes: &[u8], values: ArenaConfig) -> Result<Self> {
        let version = Self {
            hash: values.hash()?,
            source_sha256: hex::encode(Sha256::digest(bytes)),
            tanu_revision: Some(kitu_data_tmd::tables::TANU_REVISION.into()),
            provenance: None,
            values,
        };
        version.validate()?;
        Ok(version)
    }

    /// Verifies detached metadata and values, without doing document I/O.
    ///
    /// # Errors
    /// Rejects incompatible evaluator metadata, altered values or invalid hashes.
    pub fn validate(&self) -> Result<()> {
        if let Some(provenance) = &self.provenance {
            ensure!(
                self.tanu_revision.is_none(),
                "layered sources must use their explicit evaluators"
            );
            provenance.validate(&self.values)?;
            ensure!(
                self.source_sha256 == provenance.digest()?,
                "source stack digest mismatch"
            );
        } else {
            ensure!(
                self.tanu_revision.as_deref() == Some(kitu_data_tmd::tables::TANU_REVISION),
                "incompatible Tanu evaluator revision"
            );
        }
        ensure!(
            self.source_sha256.len() == 64
                && self.source_sha256.bytes().all(|c| c.is_ascii_hexdigit()),
            "invalid source SHA-256"
        );
        ensure!(
            self.hash == self.values.hash()?,
            "evaluated content hash mismatch"
        );
        ensure!(
            serde_json::to_vec(self)?.len() <= MAX_CONTENT_BYTES,
            "detached content exceeds 128 KiB"
        );
        Ok(())
    }
}

/// Current content state; staging never changes the active run's values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentSnapshot {
    /// Session-local run number, incremented by every successful start/retry.
    pub run: u64,
    /// Configuration fixed for the current or most recently completed run.
    pub active: Option<ContentVersion>,
    /// Last valid candidate to use at the next start.
    pub pending: ContentVersion,
}
use std::collections::{BTreeMap, BTreeSet};

/// One item archetype. Live item instances retain their own ID and values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemRule {
    /// Stable key referenced by chest entries; `starter` names the initial weapon.
    pub id: String,
    /// Unique displayed name.
    pub name: String,
    /// Reference ItemKind discriminant, 0 through 7.
    pub kind: i32,
    /// Preparation damage (grenade damage is also fixed at item creation).
    pub damage: i32,
    /// Repeating boss chest damage.
    pub boss_damage: i32,
    /// Weapon cooldown in seconds; zero for non-weapons.
    pub interval: f32,
}

/// Base enemy parameters before per-floor scaling.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EnemyRule {
    /// Pursuer=0, shooter=1, heavy=2, boss=3.
    pub kind: i32,
    /// Base HP.
    pub health: i32,
    /// Base attack damage.
    pub damage: i32,
    /// Ground units per second.
    pub speed: f32,
    /// Attack distance in ground units.
    pub range: f32,
    /// Attack interval in seconds.
    pub interval: f32,
    /// Collision radius in ground units.
    pub radius: f32,
}

/// Per-floor growth, preserving the reference arithmetic order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Difficulty {
    /// HP multiplier increase per floor after floor one.
    pub health_growth: f32,
    /// Attack multiplier increase per floor after floor one.
    pub damage_growth: f32,
}

/// Chest generation point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChestPhase {
    /// The initial supply chest.
    Preparing,
    /// Every living boss clear.
    Boss,
}

/// An ordered chest entry; repeated archetypes create distinct item instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChestEntry {
    /// Generation point.
    pub phase: ChestPhase,
    /// Referenced item archetype key.
    pub item_id: String,
}

/// Fully evaluated, validated content for one Arena run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArenaConfig {
    /// Typed content schema, independent of transport schema.
    pub schema_version: u32,
    /// Item definitions, including `starter`.
    pub items: Vec<ItemRule>,
    /// Exactly one definition for each enemy kind.
    pub enemies: Vec<EnemyRule>,
    /// Floor scaling.
    pub difficulty: Difficulty,
    /// Ordered chest contents.
    pub chests: Vec<ChestEntry>,
}

impl Default for ArenaConfig {
    fn default() -> Self {
        serde_json::from_str(include_str!("../../content/arena-default.json"))
            .expect("checked reference config")
    }
}

impl ArenaConfig {
    /// Evaluates all required tables through the real Tanu API, then validates
    /// their exact typed game contract. Whole-valued Formula reals are accepted
    /// for integer statistics; text-to-number coercion is never performed.
    ///
    /// # Errors
    /// Rejects malformed containers, missing/unknown tables, Formula errors,
    /// invalid identities, types, ranges or dangling chest references.
    pub fn from_tmd(bytes: &[u8]) -> Result<Self> {
        Self::from_tables(sources::tmd_tables(bytes)?)
    }

    /// Validates complete typed tables independently of their storage format.
    ///
    /// # Errors
    /// Rejects missing/unknown columns or tables, invalid scalar types, identities,
    /// ranges and references using exactly the same contract as Tanu content.
    pub fn from_tables(mut tables: ArenaTables) -> Result<Self> {
        ensure!(
            tables.keys().map(String::as_str).collect::<Vec<_>>()
                == ["chests", "difficulty", "enemies", "items"],
            "Arena requires exactly chests, difficulty, enemies and items tables"
        );
        let items = rows::<ItemRule>(
            tables.remove("items").expect("checked table"),
            "items",
            ITEM_COLUMNS,
        )?;
        let enemies = rows::<EnemyRule>(
            tables.remove("enemies").expect("checked table"),
            "enemies",
            ENEMY_COLUMNS,
        )?;
        let mut difficulty = rows::<Difficulty>(
            tables.remove("difficulty").expect("checked table"),
            "difficulty",
            DIFFICULTY_COLUMNS,
        )?;
        ensure!(difficulty.len() == 1, "difficulty requires exactly one row");
        let chests = rows::<ChestEntry>(
            tables.remove("chests").expect("checked table"),
            "chests",
            CHEST_COLUMNS,
        )?;
        let config = Self {
            schema_version: 1,
            items,
            enemies,
            difficulty: difficulty.remove(0),
            chests,
        };
        config.validate()?;
        Ok(config)
    }

    /// Checks the complete configuration before it can become pending or active.
    ///
    /// # Errors
    /// Gives an actionable table/field error without changing any running game.
    pub fn validate(&self) -> Result<()> {
        ensure!(self.schema_version == 1, "unsupported Arena content schema");
        ensure!(
            !self.items.is_empty() && self.items.len() <= 64,
            "items requires 1..64 rows"
        );
        let mut ids = BTreeSet::new();
        let mut names = BTreeSet::new();
        for item in &self.items {
            ensure!(
                !item.id.is_empty() && item.id.len() <= 64 && ids.insert(&item.id),
                "items has an empty, duplicate or oversized id: {}",
                item.id
            );
            ensure!(
                !item.name.is_empty() && item.name.len() <= 120 && names.insert(&item.name),
                "items/{} has an empty, duplicate or oversized name",
                item.id
            );
            ensure!(
                (0..=7).contains(&item.kind),
                "items/{}: kind must be 0..7",
                item.id
            );
            let minimum = if item.kind <= 2 || item.kind == 4 {
                1
            } else {
                0
            };
            ensure!(
                (minimum..=10000).contains(&item.damage)
                    && (minimum..=10000).contains(&item.boss_damage),
                "items/{}: damage out of range",
                item.id
            );
            number(
                item.interval,
                if item.kind <= 2 { 0.05 } else { 0.0 },
                60.0,
                &format!("items/{}/interval", item.id),
            )?;
        }
        ensure!(
            self.items
                .iter()
                .any(|item| item.id == "starter" && item.kind <= 2),
            "items requires a starter weapon"
        );
        ensure!(
            self.enemies.len() == 4
                && self.enemies.iter().map(|e| e.kind).collect::<BTreeSet<_>>()
                    == BTreeSet::from([0, 1, 2, 3]),
            "enemies requires exactly kinds 0,1,2,3"
        );
        for enemy in &self.enemies {
            ensure!(
                (1..=1000000).contains(&enemy.health),
                "enemies/{}: health out of range",
                enemy.kind
            );
            ensure!(
                (1..=10000).contains(&enemy.damage),
                "enemies/{}: damage out of range",
                enemy.kind
            );
            for (name, value, min, max) in [
                ("speed", enemy.speed, 0.0, 30.0),
                ("range", enemy.range, 0.0, 30.0),
                ("interval", enemy.interval, 0.05, 60.0),
                ("radius", enemy.radius, 0.05, 3.0),
            ] {
                number(value, min, max, &format!("enemies/{}/{name}", enemy.kind))?;
            }
        }
        number(
            self.difficulty.health_growth,
            0.0,
            2.0,
            "difficulty/healthGrowth",
        )?;
        number(
            self.difficulty.damage_growth,
            0.0,
            2.0,
            "difficulty/damageGrowth",
        )?;
        ensure!(self.chests.len() <= 128, "chests exceeds 128 entries");
        for (index, chest) in self.chests.iter().enumerate() {
            ensure!(
                ids.contains(&chest.item_id),
                "chests/{index}: missing item {}",
                chest.item_id
            );
        }
        Ok(())
    }

    /// Hashes canonical evaluated values, excluding container timestamps/UUIDs.
    ///
    /// # Errors
    /// Rejects invalid content before hashing.
    pub fn hash(&self) -> Result<String> {
        self.validate()?;
        Ok(hex::encode(Sha256::digest(serde_json::to_vec(self)?)))
    }

    /// Exports validated scalar tables for app-owned authoring tools.
    ///
    /// ```
    /// use kitu_demo_game::arena::config::ArenaConfig;
    /// let values = ArenaConfig::default();
    /// let tables = values.to_tables()?;
    /// assert_eq!(ArenaConfig::from_tables(tables)?, values);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    ///
    /// # Errors
    /// Rejects invalid configuration before producing a table snapshot.
    pub fn to_tables(&self) -> Result<ArenaTables> {
        self.validate()?;
        layers::tables_from_values(self)
    }

    /// Creates the editable reference document with real managed Formula tables.
    ///
    /// # Errors
    /// Returns configuration, source-definition or container-writing errors.
    pub fn to_tmd(&self) -> Result<Vec<u8>> {
        self.validate()?;
        let mut sources: BTreeMap<String, DataSourceDefinition> = BTreeMap::new();
        sources.insert(
            "items".into(),
            source(decimal_rows(&self.items)?, ITEM_COLUMNS)?,
        );
        sources.insert(
            "enemies".into(),
            source(decimal_rows(&self.enemies)?, ENEMY_COLUMNS)?,
        );
        sources.insert(
            "difficulty".into(),
            source(decimal_rows([&self.difficulty])?, DIFFICULTY_COLUMNS)?,
        );
        sources.insert(
            "chests".into(),
            source(decimal_rows(&self.chests)?, CHEST_COLUMNS)?,
        );
        let mut markdown = "# Endless Arena parameters\n\nSave edits, validate in Kitu Admin, then stage for the next run. Active runs keep their evaluated values.\n\n".to_owned();
        for name in sources.keys() {
            markdown.push_str(&format!(
                "## {name}\n\n```tmd-view:table\nsource = \"{name}\"\n```\n\n"
            ));
        }
        Ok(TanuDocument::create(markdown, sources)?.bytes()?)
    }

    pub(super) fn item(&self, id: &str) -> &ItemRule {
        self.items
            .iter()
            .find(|i| i.id == id)
            .expect("validated item key")
    }
    pub(super) fn enemy(&self, kind: i32) -> &EnemyRule {
        self.enemies
            .iter()
            .find(|e| e.kind == kind)
            .expect("validated enemy kind")
    }
}

fn number(value: f32, min: f32, max: f32, path: &str) -> Result<()> {
    ensure!(
        value.is_finite() && (min..=max).contains(&value),
        "{path}: expected finite number in {min}..{max}"
    );
    Ok(())
}

fn rows<T: serde::de::DeserializeOwned>(
    table: DataTable,
    name: &str,
    columns: &[&str],
) -> Result<Vec<T>> {
    ensure!(
        table.columns.iter().collect::<BTreeSet<_>>().len() == table.columns.len(),
        "{name}: duplicate columns"
    );
    ensure!(
        table
            .columns
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>()
            == columns.iter().copied().collect::<BTreeSet<_>>(),
        "{name}: required columns are {columns:?}"
    );
    table
        .rows
        .into_iter()
        .enumerate()
        .map(|(index, row)| {
            ensure!(
                row.len() == table.columns.len(),
                "{name}/{index}: column count mismatch"
            );
            let mut values = serde_json::Map::new();
            for (key, value) in table.columns.iter().zip(row) {
                let value = match value {
                    DataScalar::Null => serde_json::Value::Null,
                    DataScalar::Boolean(v) => v.into(),
                    DataScalar::String(v) => v.into(),
                    DataScalar::Integer(v) => v.into(),
                    DataScalar::Real(v)
                        if v.is_finite()
                            && v.fract() == 0.0
                            && v.abs() < 9_007_199_254_740_992.0 =>
                    {
                        (v as i64).into()
                    }
                    DataScalar::Real(v) if v.is_finite() => serde_json::json!(v),
                    _ => bail!("{name}/{index}/{key}: non-finite value"),
                };
                values.insert(key.clone(), value);
            }
            serde_json::from_value(values.into()).with_context(|| format!("{name}/row/{index}"))
        })
        .collect()
}

const ITEM_COLUMNS: &[&str] = &["id", "name", "kind", "damage", "bossDamage", "interval"];
const ENEMY_COLUMNS: &[&str] = &[
    "kind", "health", "damage", "speed", "range", "interval", "radius",
];
const DIFFICULTY_COLUMNS: &[&str] = &["healthGrowth", "damageGrowth"];
const CHEST_COLUMNS: &[&str] = &["phase", "itemId"];

// Preserve the shortest round-tripping f32 decimal in editable cells instead
// of exposing the binary expansion produced by converting directly to f64.
fn decimal_rows(value: impl Serialize) -> Result<serde_json::Value> {
    Ok(serde_json::from_slice(&serde_json::to_vec(&value)?)?)
}

fn source(rows: serde_json::Value, column_names: &[&str]) -> Result<DataSourceDefinition> {
    let rows = rows.as_array().context("table rows must be an array")?;
    let columns = column_names
        .iter()
        .map(|name| FormulaTableColumn {
            id: (*name).into(),
            name: (*name).into(),
            constraint: if ["id", "name", "phase", "itemId"].contains(name) {
                FormulaCellConstraint::Text
            } else {
                FormulaCellConstraint::Number
            },
            identity: false,
            hidden: false,
            reference: None,
        })
        .collect::<Vec<_>>();
    let rows = rows
        .iter()
        .enumerate()
        .map(|(index, row)| {
            let cells = columns
                .iter()
                .map(|column| {
                    let value = &row[&column.name];
                    let literal = if let Some(v) = value.as_str() {
                        FormulaTableLiteral::String { value: v.into() }
                    } else if let Some(v) = value.as_i64() {
                        FormulaTableLiteral::Integer { value: v }
                    } else {
                        FormulaTableLiteral::Real {
                            value: value.as_f64().expect("typed numeric field"),
                        }
                    };
                    FormulaTableCell {
                        content: FormulaTableCellContent::Literal { value: literal },
                        constraint: None,
                    }
                })
                .collect();
            FormulaTableRow {
                id: format!("row-{index}"),
                cells,
            }
        })
        .collect();
    Ok(DataSourceDefinition::FormulaTable {
        columns,
        rows,
        reference_groups: Vec::new(),
    })
}
