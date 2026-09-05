//! Real Tanu containers and evaluated tables, using the pinned public Tanu API.
//!
//! Parsing, Formula semantics and document editing remain owned by Tanu. This
//! boundary never interprets a `.tmd` file as the legacy key/value helper format.

use std::{collections::BTreeMap, io::Cursor};
pub use tmd_core::{
    DataCellEdit, DataScalar, DataSourceDefinition, DataSourceRegistry, DataTable,
    FormulaCellConstraint, FormulaTableCell, FormulaTableCellContent, FormulaTableColumn,
    FormulaTableLiteral, FormulaTableRow, TmdError, TmdResult,
};
use tmd_core::{ReadMode, TmdDoc, WriteMode, DATA_SOURCES_EXTRAS_KEY};

/// Source revision defining container and Formula compatibility.
pub const TANU_REVISION: &str = "194358e8791f1391492abcb60d8cfcc37bbb383a";

/// A loaded Tanu document. Evaluate it outside the gameplay tick, then pass
/// detached values to the application for typed validation and activation.
pub struct TanuDocument {
    doc: TmdDoc,
}

impl TanuDocument {
    /// Reads and validates a real ZIP-based TMD container.
    ///
    /// # Errors
    /// Returns Tanu's structured error for malformed or invalid documents.
    pub fn read(bytes: &[u8]) -> TmdResult<Self> {
        if bytes.len() > 16 * 1024 * 1024 {
            return Err(TmdError::InvalidFormat(
                "Kitu TMD input exceeds 16 MiB".into(),
            ));
        }
        let doc = tmd_core::read_tmd(&mut Cursor::new(bytes), ReadMode::default())?;
        let report = tmd_core::validate_document(&doc)?;
        if report
            .issues
            .iter()
            .any(|issue| issue.severity == tmd_core::ValidationSeverity::Error)
        {
            return Err(TmdError::InvalidFormat(format!(
                "document validation: {:?}",
                report.issues
            )));
        }
        Ok(Self { doc })
    }

    /// Builds a document with managed Formula sources using Tanu's data model.
    ///
    /// # Errors
    /// Returns an error if the document or source registry is invalid.
    pub fn create(
        markdown: String,
        sources: BTreeMap<String, DataSourceDefinition>,
    ) -> TmdResult<Self> {
        let mut doc = TmdDoc::new(markdown)?;
        let registry = DataSourceRegistry {
            sources,
            ..DataSourceRegistry::default()
        };
        doc.manifest.extras = serde_json::json!({(DATA_SOURCES_EXTRAS_KEY): registry});
        DataSourceRegistry::from_manifest_extras(&doc.manifest.extras)?;
        Ok(Self { doc })
    }

    /// Evaluates a named table through the actual Tanu Formula/data-source API.
    ///
    /// # Errors
    /// Rejects unknown sources, Formula failures and scalar-shaped outputs.
    pub fn table(&self, name: &str) -> TmdResult<DataTable> {
        match tmd_core::evaluate_data_source(&self.doc, name)? {
            tmd_core::DataValue::Table(table) => Ok(table),
            _ => Err(TmdError::DataView(format!(
                "source `{name}` must be a table"
            ))),
        }
    }

    /// Lists all declared source names in stable order.
    ///
    /// # Errors
    /// Returns Tanu's registry validation errors.
    pub fn source_names(&self) -> TmdResult<Vec<String>> {
        Ok(
            DataSourceRegistry::from_manifest_extras(&self.doc.manifest.extras)?
                .sources
                .into_keys()
                .collect(),
        )
    }

    /// Applies Tanu's public cell-edit operation for editable Formula-query tables.
    /// Managed Formula tables are edited through their typed source definitions.
    ///
    /// # Errors
    /// Returns cell identity, type, Formula or write-back errors from Tanu.
    pub fn edit(&mut self, edits: &[DataCellEdit]) -> TmdResult<()> {
        tmd_core::apply_data_cell_edits(&mut self.doc, edits)
    }

    /// Returns the typed source definitions for authoring tools.
    ///
    /// # Errors
    /// Returns registry validation errors from Tanu.
    pub fn sources(&self) -> TmdResult<DataSourceRegistry> {
        DataSourceRegistry::from_manifest_extras(&self.doc.manifest.extras)
    }

    /// Replaces source definitions after Tanu validates the entire registry.
    ///
    /// # Errors
    /// Invalid definitions leave the current document unchanged. Formula
    /// evaluation is a separate step so editors can retain diagnostic drafts.
    pub fn set_sources(&mut self, sources: DataSourceRegistry) -> TmdResult<()> {
        let mut extras = self.doc.manifest.extras.clone();
        extras[DATA_SOURCES_EXTRAS_KEY] = serde_json::to_value(sources)?;
        DataSourceRegistry::from_manifest_extras(&extras)?;
        self.doc.manifest.extras = extras;
        Ok(())
    }

    /// Serializes the document with Tanu's container writer.
    ///
    /// # Errors
    /// Returns Tanu's serialization, validation or I/O errors.
    ///
    /// # Examples
    /// ```
    /// use kitu_data_tmd::tables::TanuDocument;
    /// let document = TanuDocument::create("# Settings".into(), Default::default()).unwrap();
    /// let bytes = document.bytes().unwrap();
    /// assert_eq!(&bytes[..2], b"PK");
    /// assert!(TanuDocument::read(&bytes).unwrap().source_names().unwrap().is_empty());
    /// ```
    pub fn bytes(&self) -> TmdResult<Vec<u8>> {
        let mut output = Cursor::new(Vec::new());
        tmd_core::write_tmd(&mut output, &self.doc, WriteMode::default())?;
        Ok(output.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sources() -> BTreeMap<String, DataSourceDefinition> {
        let registry: DataSourceRegistry = serde_json::from_value(serde_json::json!({
            "schema_version":8,"sources":{"weapons":{
                "type":"formula",
                "columns":[{"id":"damage","name":"damage","constraint":"number"},{"id":"boss","name":"boss","constraint":"number"}],
                "rows":[{"id":"blade","cells":[
                    {"content":{"kind":"literal","value":{"type":"integer","value":"20"}}},
                    {"content":{"kind":"formula","expression":"A1 + 4"}}
                ]}]
            }}
        })).unwrap();
        registry.sources
    }

    #[test]
    fn real_container_formula_and_managed_definition_edits_round_trip() {
        let mut doc = TanuDocument::create("# Weapons".into(), sources()).unwrap();
        let bytes = doc.bytes().unwrap();
        assert_eq!(&bytes[..2], b"PK");
        let initial = TanuDocument::read(&bytes).unwrap();
        assert_eq!(initial.source_names().unwrap(), ["weapons"]);
        assert_eq!(
            initial.table("weapons").unwrap().rows[0][1],
            DataScalar::Integer(24)
        );
        let mut registry = doc.sources().unwrap();
        let DataSourceDefinition::FormulaTable { rows, .. } =
            registry.sources.get_mut("weapons").unwrap()
        else {
            panic!("managed table")
        };
        rows[0].cells[0].content = FormulaTableCellContent::Literal {
            value: FormulaTableLiteral::Integer { value: 30 },
        };
        doc.set_sources(registry).unwrap();
        let edited = TanuDocument::read(&doc.bytes().unwrap()).unwrap();
        assert_eq!(
            edited.table("weapons").unwrap().rows[0][1],
            DataScalar::Integer(34)
        );
        assert_eq!(
            initial.table("weapons").unwrap().rows[0][1],
            DataScalar::Integer(24)
        );
        assert!(doc.table("missing").is_err());
    }

    #[test]
    fn malformed_inputs_and_invalid_registry_changes_do_not_silently_fall_back() {
        assert!(TanuDocument::read(b"hp: 100").is_err());
        let mut doc = TanuDocument::create("# Weapons".into(), sources()).unwrap();
        let mut registry = doc.sources().unwrap();
        let DataSourceDefinition::FormulaTable { columns, .. } =
            registry.sources.get_mut("weapons").unwrap()
        else {
            panic!("managed table")
        };
        columns[1].id = columns[0].id.clone();
        assert!(doc.set_sources(registry).is_err());
        assert_eq!(
            doc.table("weapons").unwrap().rows[0][1],
            DataScalar::Integer(24)
        );
        assert!(doc
            .edit(&[DataCellEdit {
                source: "weapons".into(),
                key: DataScalar::String("blade".into()),
                column: "damage".into(),
                value: DataScalar::Integer(99)
            }])
            .is_err());
        assert!(doc.edit(&[]).is_ok());
        let mut bytes = doc.bytes().unwrap();
        bytes.truncate(bytes.len() / 2);
        assert!(TanuDocument::read(&bytes).is_err());
    }
}
