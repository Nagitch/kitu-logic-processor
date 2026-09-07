//! Bounded, read-only SQLite snapshots for application-owned typed data.
//!
//! Applications specify tables, allowed/required columns and explicit integer
//! ordering keys. Schema discovery and all data reads share one transaction,
//! including committed WAL data. Returned values are detached from the file.
//! This crate does not own game schemas, authoring, Formula evaluation or reload
//! activation. It replaces the early in-memory `SqliteStore` placeholder.

#![warn(missing_docs)]

use serde::Serialize;
use std::{path::Path, time::Duration};
use thiserror::Error;

mod reader;

/// Scalar storage values without string/number or affinity coercion in Rust.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum Scalar {
    /// SQL NULL; the application decides whether its field permits it.
    Null,
    /// Exact signed 64-bit SQLite integer.
    Integer(i64),
    /// Finite SQLite double; non-finite stored values are rejected.
    Real(f64),
    /// Valid UTF-8 text, bounded before allocation.
    Text(String),
}

/// One application-selected table and its deterministic projection.
#[derive(Debug, Clone)]
pub struct TableSpec {
    /// Exact table name; ASCII letter/underscore followed by letters/digits/underscores.
    pub name: String,
    /// Allowed data columns, also defining returned column order. Missing optional
    /// columns are omitted; unknown database columns are rejected.
    pub columns: Vec<String>,
    /// Subset of `columns` that must exist even in a sparse patch table.
    pub required_columns: Vec<String>,
    /// Nonempty ordering tuple, ascending. Every key must exist and contain a
    /// non-null INTEGER. Duplicate tuples are rejected instead of using an
    /// implicit row order. Keys need not appear in returned data columns.
    pub order_by: Vec<String>,
    /// An absent optional table is omitted. A present empty table is retained,
    /// allowing applications to distinguish omission from clearing.
    pub optional: bool,
}

/// Detached table with deterministic column and row order.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TableSnapshot {
    /// Table name copied from its specification.
    pub name: String,
    /// Present allowed columns in specification order.
    pub columns: Vec<String>,
    /// Explicit ordering column names, included in source provenance.
    pub order_by: Vec<String>,
    /// Ordering tuples aligned with `rows`, including keys excluded from data columns.
    pub order_keys: Vec<Vec<i64>>,
    /// Native scalar values aligned with `columns`, sorted by the explicit keys.
    pub rows: Vec<Vec<Scalar>>,
}

/// One consistent database view, independent of later writes or file removal.
/// Hash a canonical serialization of this value for source provenance: hashing
/// only the main database file would miss committed WAL changes.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    /// Application schema marker from `PRAGMA user_version` in the transaction.
    pub schema_version: u32,
    /// Present tables in specification order.
    pub tables: Vec<TableSnapshot>,
}

/// Explicit limits for materialization and SQLite execution.
#[derive(Debug, Clone)]
pub struct ReadOptions {
    /// Required `user_version`, or `None` to report any nonnegative version.
    pub expected_schema_version: Option<u32>,
    /// Reject application tables/views absent from the specification. Internal
    /// SQLite tables, indexes and triggers do not count as application tables.
    pub deny_unknown_tables: bool,
    /// Maximum requested tables and discovered application tables/views.
    pub max_tables: usize,
    /// Maximum allowed/projected columns plus ordering keys per table.
    pub max_columns: usize,
    /// Maximum rows per present table (zero permits only empty tables).
    pub max_rows_per_table: usize,
    /// Aggregate returned cells plus ordering-key cells examined.
    pub max_total_cells: usize,
    /// Maximum bytes per scalar (INTEGER/REAL count as eight, NULL as zero).
    pub max_value_bytes: usize,
    /// Aggregate scalar bytes including ordering keys; table/column identifiers
    /// also count. SQLite additionally caps individual row allocations.
    pub max_total_bytes: usize,
    /// Maximum aggregate VM operations across discovery/reads. Progress is
    /// checked every at most 1,000 operations.
    pub max_vm_steps: u64,
    /// Maximum lock wait, also the maximum cancellation delay in SQLite's busy
    /// handler. Restricted to at most five seconds.
    pub busy_timeout: Duration,
}

impl Default for ReadOptions {
    fn default() -> Self {
        Self {
            expected_schema_version: None,
            deny_unknown_tables: true,
            max_tables: 64,
            max_columns: 128,
            max_rows_per_table: 100_000,
            max_total_cells: 1_000_000,
            max_value_bytes: 1_048_576,
            max_total_bytes: 16 * 1_048_576,
            max_vm_steps: 10_000_000,
            busy_timeout: Duration::from_millis(100),
        }
    }
}

/// Snapshot failures never return partially read data.
#[derive(Debug, Error)]
pub enum SqliteError {
    /// Invalid identifiers, duplicate specifications or inconsistent limits.
    #[error("invalid SQLite read specification: {0}")]
    InvalidSpec(String),
    /// Missing/unexpected tables/columns, schema mismatch or ambiguous ordering.
    #[error("invalid SQLite schema: {0}")]
    InvalidSchema(String),
    /// A value violates the scalar or explicit ordering contract.
    #[error("invalid SQLite value at {table}/{row}/{column}: {reason}")]
    InvalidValue {
        /// Source table.
        table: String,
        /// Zero-based row in the ordered result.
        row: usize,
        /// Source column.
        column: String,
        /// Stable human-readable rejection reason.
        reason: &'static str,
    },
    /// A configured materialization or SQL execution bound was reached.
    #[error("SQLite snapshot exceeds {0} limit")]
    LimitExceeded(&'static str),
    /// Caller-requested cancellation.
    #[error("SQLite snapshot cancelled")]
    Cancelled,
    /// A cancellation callback panicked; it never unwinds through SQLite's C ABI.
    #[error("SQLite cancellation callback panicked")]
    CancellationCallbackPanicked,
    /// Filesystem access failed before opening the read-only database.
    #[error("SQLite source access failed: {0}")]
    Io(#[from] std::io::Error),
    /// SQLite failed to open, parse or execute a read operation.
    #[error("SQLite read failed: {0}")]
    Database(#[from] rusqlite::Error),
}

/// Result of an application-independent SQLite read.
pub type Result<T> = std::result::Result<T, SqliteError>;

/// Reads specified tables in a single read-only transaction.
///
/// The callback runs synchronously between reads and in SQLite's VM progress
/// hook. It must return promptly. Returning `true` cancels the entire snapshot.
/// Lock waits are bounded by `busy_timeout`; cancellation does not interrupt an
/// operating-system file read. No connection or file-backed values escape.
///
/// # Errors
/// Rejects invalid specifications, unsupported schema/scalars, exceeded limits,
/// cancellation, database locks/corruption and filesystem errors. Sources must
/// be existing regular files; a missing path is never created.
///
/// # Examples
/// ```no_run
/// use kitu_data_sqlite::{read_snapshot, ReadOptions, TableSpec};
/// let tables = [TableSpec {
///     name: "items".into(), columns: vec!["id".into(), "damage".into()],
///     required_columns: vec!["id".into(), "damage".into()],
///     order_by: vec!["ordinal".into()], optional: false,
/// }];
/// let snapshot = read_snapshot(std::path::Path::new("arena.sqlite"), &tables,
///     &ReadOptions::default(), || false)?;
/// assert_eq!(snapshot.tables[0].columns, ["id", "damage"]);
/// # Ok::<(), kitu_data_sqlite::SqliteError>(())
/// ```
pub fn read_snapshot(
    path: &Path,
    tables: &[TableSpec],
    options: &ReadOptions,
    cancelled: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<Snapshot> {
    reader::read(path, tables, options, cancelled)
}

#[cfg(test)]
mod tests;
