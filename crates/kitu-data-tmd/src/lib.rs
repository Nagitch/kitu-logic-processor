//! Data ingestion helpers for the TMD format.
//!
//! # Responsibilities
//! - Parse authored TMD content into strongly typed structures ready for validation.
//! - Keep schema evolution localized so runtime and tools consume a consistent model.
//! - Provide adapters for loading parsed data into storage or runtime-facing structures.
//!
//! # Integration
//! This crate feeds authored data into persistence (`kitu-data-sqlite`) and runtime layers. Refer to
//! `doc/crates-overview.md` for how TMD ingestion fits alongside timelines and transports.

use std::collections::HashMap;

use kitu_core::{KituError, Result};

/// Minimal representation of a TMD entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TmdEntry {
    /// Key portion from the `key: value` line.
    pub key: String,
    /// Value parsed from the input document for the given key.
    pub value: String,
}

/// Parsed TMD document as a key-value map.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TmdDocument {
    entries: HashMap<String, String>,
}

impl TmdDocument {
    /// Parses a simple `key: value` style document.
    pub fn parse(input: &str) -> Result<Self> {
        let mut entries = HashMap::new();
        for line in input.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = trimmed.split_once(':') {
                entries.insert(key.trim().to_string(), value.trim().to_string());
            } else {
                return Err(KituError::InvalidInput("expected key: value"));
            }
        }
        Ok(Self { entries })
    }

    /// Retrieves a parsed entry by key.
    pub fn get(&self, key: &str) -> Option<TmdEntry> {
        self.entries.get(key).cloned().map(|value| TmdEntry {
            key: key.to_string(),
            value,
        })
    }

    /// Returns how many entries were parsed.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if no entries were parsed.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_key_value_pairs() {
        let doc = TmdDocument::parse("hp: 10\nname: hero").unwrap();
        assert_eq!(doc.len(), 2);
        assert_eq!(doc.get("hp").unwrap().value, "10");
    }

    #[test]
    fn parsing_invalid_line_returns_error() {
        let result = TmdDocument::parse("invalid line");
        assert!(matches!(result, Err(KituError::InvalidInput(_))));
    }
}
