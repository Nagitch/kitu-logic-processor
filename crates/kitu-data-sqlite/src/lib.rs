//! SQLite data access layer placeholder.
//!
//! # Responsibilities
//! - Define the SQLite schema helpers and migrations shared by runtime and tooling.
//! - Expose lightweight query utilities so consumers do not duplicate ad-hoc SQL.
//! - Remain agnostic to higher-level gameplay logic, focusing only on persistence concerns.
//!
//! # Integration
//! This crate sits beneath data ingestion (`kitu-data-tmd`) and runtime consumers that require a
//! stable storage layer. See `doc/crates-overview.md` for how data crates connect to the runtime.

use std::collections::HashMap;

use kitu_core::{KituError, Result};

/// Represents a simplistic table as a list of rows (column -> value).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Table {
    /// Identifier for the table, usually the underlying SQLite table name.
    pub name: String,
    /// Rows stored in insertion order, represented as column-to-value maps.
    pub rows: Vec<HashMap<String, String>>,
}

/// In-memory stand-in for an actual SQLite connection.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SqliteStore {
    tables: HashMap<String, Table>,
}

impl SqliteStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a new empty table.
    pub fn create_table(&mut self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        if self.tables.contains_key(&name) {
            return Err(KituError::InvalidInput("table already exists"));
        }
        self.tables.insert(
            name.clone(),
            Table {
                name,
                rows: Vec::new(),
            },
        );
        Ok(())
    }

    /// Inserts a row into an existing table.
    pub fn insert(&mut self, table: &str, row: HashMap<String, String>) -> Result<()> {
        let table = self
            .tables
            .get_mut(table)
            .ok_or(KituError::InvalidInput("missing table"))?;
        table.rows.push(row);
        Ok(())
    }

    /// Simple query returning all rows for a table.
    pub fn query_all(&self, table: &str) -> Result<&[HashMap<String, String>]> {
        let table = self
            .tables
            .get(table)
            .ok_or(KituError::InvalidInput("missing table"))?;
        Ok(&table.rows)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creating_tables_and_inserting_rows() {
        let mut store = SqliteStore::new();
        store.create_table("items").unwrap();

        let mut row = HashMap::new();
        row.insert("id".into(), "1".into());
        row.insert("name".into(), "potion".into());
        store.insert("items", row.clone()).unwrap();

        let rows = store.query_all("items").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].get("name").unwrap(), "potion");
    }

    #[test]
    fn duplicate_table_creation_is_an_error() {
        let mut store = SqliteStore::new();
        store.create_table("dupe").unwrap();
        assert!(store.create_table("dupe").is_err());
    }
}
