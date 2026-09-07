#[allow(dead_code)]
mod support;
use kitu_data_tmd::tables::{
    DataScalar, DataSourceDefinition, DataTable, FormulaCellConstraint, FormulaTableCell,
    FormulaTableCellContent, FormulaTableColumn, FormulaTableLiteral, FormulaTableRow,
    TanuDocument,
};
use kitu_demo_game::{
    arena::{self, config::*},
    build_demo_runtime,
    replay::{Recorder, Session},
};
use kitu_osc_ir::{OscArg, OscBundle};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "arena-sources-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn table(columns: &[&str], rows: Vec<Vec<DataScalar>>) -> DataTable {
    DataTable {
        columns: columns.iter().map(|name| (*name).into()).collect(),
        rows,
    }
}
fn patch(name: &str, columns: &[&str], rows: Vec<Vec<DataScalar>>) -> ArenaTables {
    BTreeMap::from([(name.into(), table(columns, rows))])
}
fn text(value: &str) -> DataScalar {
    DataScalar::String(value.into())
}
fn integer(value: i64) -> DataScalar {
    DataScalar::Integer(value)
}
fn plan(base: &str, format: SourceFormat, patch: &str) -> SourcePlan {
    SourcePlan {
        version: 1,
        base: SourceReference {
            format,
            path: base.into(),
        },
        difficulty: None,
        event: None,
        debug: Some(SourceReference {
            format: SourceFormat::Sqlite,
            path: patch.into(),
        }),
    }
}
fn save_plan(path: &Path, plan: &SourcePlan) {
    std::fs::write(path, serde_json::to_vec(plan).unwrap()).unwrap();
}

#[test]
fn sqlite_values_match_real_tanu_and_legacy_json_keeps_its_contract() {
    let dir = Directory::new();
    let config = ArenaConfig::default();
    write_sqlite(&dir.path("base.sqlite"), &config.to_tables().unwrap()).unwrap();
    let sqlite = load_content(&dir.path("base.sqlite")).unwrap().version;
    let tmd = ContentVersion::from_tmd(include_bytes!("../content/arena.tmd")).unwrap();
    assert_eq!(sqlite.values, tmd.values);
    assert_eq!(sqlite.hash, tmd.hash);
    assert_eq!(sqlite.tanu_revision, None);
    let encoded = serde_json::to_value(&tmd).unwrap();
    assert_eq!(
        encoded
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>(),
        vec!["hash", "sourceSha256", "tanuRevision", "values"]
    );
    assert_eq!(
        encoded["tanuRevision"],
        kitu_data_tmd::tables::TANU_REVISION
    );
    assert_eq!(
        content_origins(&tmd).unwrap()["/items/starter/damage"],
        Layer::Base
    );
    let provenance = sqlite.provenance.as_ref().unwrap();
    assert_eq!(provenance.sources[0].format, SourceFormat::Sqlite);
    assert!(!provenance.sources[0].evaluator.contains("tanu"));
    let mut forged = sqlite.clone();
    forged.tanu_revision = tmd.tanu_revision;
    assert!(forged.validate().is_err());
    assert!(diff_content(&sqlite, &sqlite).unwrap().is_empty());
    assert!(serde_json::to_string(&sqlite).unwrap().len() <= MAX_CONTENT_BYTES);
    let mut tables = config.to_tables().unwrap();
    tables.get_mut("items").unwrap().rows[0][3] = DataScalar::Real(20.0);
    assert_eq!(ArenaConfig::from_tables(tables).unwrap(), config);
}

#[test]
fn mixed_plan_precedence_origins_diffs_and_source_relocation_are_deterministic() {
    let dir = Directory::new();
    create_source_plan(&dir.path("sample.arena.json")).unwrap();
    let loaded = load_content(&dir.path("sample.arena.json")).unwrap();
    assert_eq!(loaded.version.values.items[0].damage, 32);
    let provenance = loaded.version.provenance.as_ref().unwrap();
    assert_eq!(
        provenance
            .sources
            .iter()
            .map(|value| value.layer)
            .collect::<Vec<_>>(),
        [Layer::Base, Layer::Difficulty, Layer::Event, Layer::Debug]
    );
    assert_eq!(
        provenance
            .sources
            .iter()
            .map(|value| value.format)
            .collect::<Vec<_>>(),
        [
            SourceFormat::Sqlite,
            SourceFormat::Tmd,
            SourceFormat::Sqlite,
            SourceFormat::Tmd
        ]
    );
    assert_eq!(
        content_origins(&loaded.version).unwrap()["/items/starter/damage"],
        Layer::Debug
    );
    let baseline = ContentVersion::from_tmd(include_bytes!("../content/arena.tmd")).unwrap();
    let changes = diff_content(&baseline, &loaded.version).unwrap();
    assert_eq!(
        changes,
        [ContentDifference {
            path: "/items/starter/damage".into(),
            before: json!(20),
            after: json!(32),
            winning_layer: Layer::Debug
        }]
    );
    assert!(!serde_json::to_string(&loaded.version)
        .unwrap()
        .contains(dir.0.to_str().unwrap()));
    let copy = Directory::new();
    for file in std::fs::read_dir(&dir.0).unwrap() {
        let file = file.unwrap();
        std::fs::copy(file.path(), copy.0.join(file.file_name())).unwrap();
    }
    assert_eq!(
        load_content(&copy.path("sample.arena.json"))
            .unwrap()
            .version,
        loaded.version
    );
    let mut plan: SourcePlan =
        serde_json::from_slice(&std::fs::read(dir.path("sample.arena.json")).unwrap()).unwrap();
    plan.debug = None;
    save_plan(&dir.path("sample.arena.json"), &plan);
    let event = load_content(&dir.path("sample.arena.json"))
        .unwrap()
        .version;
    assert_eq!(event.values.items[0].damage, 28);
    assert_eq!(
        content_origins(&event).unwrap()["/items/starter/damage"],
        Layer::Event
    );
    assert_ne!(event.source_sha256, loaded.version.source_sha256);
    plan.event = None;
    save_plan(&dir.path("sample.arena.json"), &plan);
    assert_eq!(
        load_content(&dir.path("sample.arena.json"))
            .unwrap()
            .version
            .values
            .items[0]
            .damage,
        24
    );
}

#[test]
fn sparse_sqlite_rejects_unknown_fields_identities_types_and_references_atomically() {
    let dir = Directory::new();
    std::fs::write(dir.path("base.tmd"), include_bytes!("../content/arena.tmd")).unwrap();
    let cases = [
        patch(
            "items",
            &["id", "damage"],
            vec![vec![text("missing"), integer(30)]],
        ),
        patch(
            "items",
            &["id", "damage"],
            vec![vec![text("starter"), DataScalar::Null]],
        ),
        patch(
            "items",
            &["id", "damage"],
            vec![vec![text("starter"), text("30")]],
        ),
        patch(
            "items",
            &["id", "damage"],
            vec![vec![text("starter"), integer(0)]],
        ),
        patch(
            "items",
            &["id", "damage"],
            vec![
                vec![text("starter"), integer(30)],
                vec![text("starter"), integer(31)],
            ],
        ),
        patch("items", &["damage"], vec![vec![integer(30)]]),
        patch(
            "enemies",
            &["kind", "health"],
            vec![vec![integer(4), integer(20)]],
        ),
        patch(
            "difficulty",
            &["damageGrowth"],
            vec![vec![DataScalar::Real(3.0)]],
        ),
        patch("difficulty", &["damageGrowth"], vec![]),
        patch(
            "chests",
            &["phase", "itemId"],
            vec![vec![text("boss"), text("missing")]],
        ),
        patch("chests", &["itemId"], vec![vec![text("quick-blade")]]),
    ];
    for (index, tables) in cases.iter().enumerate() {
        let name = format!("bad-{index}.sqlite");
        write_sqlite(&dir.path(&name), tables).unwrap();
        save_plan(
            &dir.path("test.arena.json"),
            &plan("base.tmd", SourceFormat::Tmd, &name),
        );
        assert!(
            load_content(&dir.path("test.arena.json")).is_err(),
            "case {index}"
        );
    }
    let base = load_content(&dir.path("base.tmd")).unwrap().version;
    assert_eq!(base.values, ArenaConfig::default());
    let mut unknown = patch("items", &["id", "damage"], vec![]);
    unknown
        .get_mut("items")
        .unwrap()
        .columns
        .push("surprise".into());
    assert!(write_sqlite(&dir.path("unknown.sqlite"), &unknown).is_err());
}

#[test]
fn chest_replacement_preserves_repetition_and_empty_replacement_and_escaped_ids() {
    let dir = Directory::new();
    let mut config = ArenaConfig::default();
    config.items[1].id = "blade/~".into();
    for entry in &mut config.chests {
        if entry.item_id == "quick-blade" {
            entry.item_id = "blade/~".into();
        }
    }
    write_sqlite(&dir.path("base.sqlite"), &config.to_tables().unwrap()).unwrap();
    let mut tables = patch(
        "items",
        &["id", "damage"],
        vec![vec![text("blade/~"), DataScalar::Real(26.0)]],
    );
    tables.insert(
        "chests".into(),
        table(
            &["phase", "itemId"],
            vec![
                vec![text("boss"), text("blade/~")],
                vec![text("boss"), text("blade/~")],
            ],
        ),
    );
    write_sqlite(&dir.path("patch.sqlite"), &tables).unwrap();
    save_plan(
        &dir.path("test.arena.json"),
        &plan("base.sqlite", SourceFormat::Sqlite, "patch.sqlite"),
    );
    let changed = load_content(&dir.path("test.arena.json")).unwrap().version;
    assert_eq!(changed.values.chests.len(), 2);
    assert_eq!(changed.values.chests[0], changed.values.chests[1]);
    assert_eq!(
        content_origins(&changed).unwrap()["/items/blade~1~0/damage"],
        Layer::Debug
    );
    assert_eq!(content_origins(&changed).unwrap()["/chests"], Layer::Debug);
    write_sqlite(
        &dir.path("empty.sqlite"),
        &patch("chests", &["phase", "itemId"], vec![]),
    )
    .unwrap();
    save_plan(
        &dir.path("test.arena.json"),
        &plan("base.sqlite", SourceFormat::Sqlite, "empty.sqlite"),
    );
    assert!(load_content(&dir.path("test.arena.json"))
        .unwrap()
        .version
        .values
        .chests
        .is_empty());
}

#[test]
fn sqlite_source_digest_includes_committed_wal_and_explicit_ordinals() {
    let dir = Directory::new();
    let path = dir.path("base.sqlite");
    write_sqlite(&path, &ArenaConfig::default().to_tables().unwrap()).unwrap();
    let database = rusqlite::Connection::open(&path).unwrap();
    database.pragma_update(None, "journal_mode", "WAL").unwrap();
    database
        .pragma_update(None, "wal_autocheckpoint", 0)
        .unwrap();
    database
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .unwrap();
    let main = std::fs::read(&path).unwrap();
    let before = load_content(&path).unwrap().version;
    database
        .execute("UPDATE items SET damage=31 WHERE id='starter'", [])
        .unwrap();
    assert_eq!(
        std::fs::read(&path).unwrap(),
        main,
        "the committed value is only in WAL"
    );
    let edited = load_content(&path).unwrap().version;
    assert_eq!(edited.values.items[0].damage, 31);
    assert_ne!(edited.source_sha256, before.source_sha256);
    database
        .execute("UPDATE items SET ordinal=ordinal+100", [])
        .unwrap();
    let reordered_keys = load_content(&path).unwrap().version;
    assert_eq!(edited.values, reordered_keys.values);
    assert_eq!(edited.hash, reordered_keys.hash);
    assert_ne!(edited.source_sha256, reordered_keys.source_sha256);
}

#[test]
fn schema_cancellation_and_plan_limits_reject_before_staging() {
    let dir = Directory::new();
    let path = dir.path("base.sqlite");
    write_sqlite(&path, &ArenaConfig::default().to_tables().unwrap()).unwrap();
    let database = rusqlite::Connection::open(&path).unwrap();
    database.pragma_update(None, "user_version", 2).unwrap();
    assert!(load_content(&path)
        .unwrap_err()
        .to_string()
        .contains("base.sqlite"));
    database.pragma_update(None, "user_version", 1).unwrap();
    database
        .execute_batch("ALTER TABLE items ADD COLUMN unknown; CREATE TABLE extra (ordinal INTEGER)")
        .unwrap();
    assert!(load_content(&path).is_err());
    assert!(load_content_with_cancel(&path, || true)
        .unwrap_err()
        .to_string()
        .contains("cancelled"));
    assert!(load_content(Path::new(""))
        .unwrap_err()
        .to_string()
        .contains("disabled"));
    std::fs::write(
        dir.path("large.arena.json"),
        vec![b' '; MAX_CONTENT_BYTES + 1],
    )
    .unwrap();
    assert!(load_content(&dir.path("large.arena.json"))
        .unwrap_err()
        .to_string()
        .contains("size limit"));
    let duplicate = br#"{"version":1,"version":1,"base":{"format":"sqlite","path":"base.sqlite"}}"#;
    std::fs::write(dir.path("duplicate.arena.json"), duplicate).unwrap();
    assert!(load_content(&dir.path("duplicate.arena.json")).is_err());
}

#[test]
fn authoring_writer_rejects_unsupported_scalars_before_creating_files() {
    let dir = Directory::new();
    for (name, value) in [
        ("boolean.sqlite", DataScalar::Boolean(true)),
        ("nan.sqlite", DataScalar::Real(f64::NAN)),
    ] {
        assert!(write_sqlite(
            &dir.path(name),
            &patch(
                "items",
                &["id", "damage"],
                vec![vec![text("starter"), value]]
            )
        )
        .is_err());
        assert!(!dir.path(name).exists());
    }
    let path = dir.path("existing.sqlite");
    std::fs::write(&path, b"retain author file").unwrap();
    assert!(write_sqlite(&path, &ArenaConfig::default().to_tables().unwrap()).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), b"retain author file");
}

#[test]
fn real_tanu_formula_in_sparse_layer_is_evaluated_before_overrides() {
    let dir = Directory::new();
    write_sqlite(
        &dir.path("base.sqlite"),
        &ArenaConfig::default().to_tables().unwrap(),
    )
    .unwrap();
    let columns = ["id", "damage"]
        .map(|name| FormulaTableColumn {
            id: name.into(),
            name: name.into(),
            constraint: if name == "id" {
                FormulaCellConstraint::Text
            } else {
                FormulaCellConstraint::Number
            },
            identity: false,
            hidden: false,
            reference: None,
        })
        .to_vec();
    let row = FormulaTableRow {
        id: "row-0".into(),
        cells: vec![
            FormulaTableCell {
                content: FormulaTableCellContent::Literal {
                    value: FormulaTableLiteral::String {
                        value: "starter".into(),
                    },
                },
                constraint: None,
            },
            FormulaTableCell {
                content: FormulaTableCellContent::Formula {
                    expression: "20 + 7".into(),
                },
                constraint: None,
            },
        ],
    };
    let document = TanuDocument::create(
        "# Sparse formula".into(),
        BTreeMap::from([(
            "items".into(),
            DataSourceDefinition::FormulaTable {
                columns,
                rows: vec![row],
                reference_groups: vec![],
            },
        )]),
    )
    .unwrap();
    std::fs::write(dir.path("formula.tmd"), document.bytes().unwrap()).unwrap();
    let plan = SourcePlan {
        version: 1,
        base: SourceReference {
            format: SourceFormat::Sqlite,
            path: "base.sqlite".into(),
        },
        difficulty: Some(SourceReference {
            format: SourceFormat::Tmd,
            path: "formula.tmd".into(),
        }),
        event: None,
        debug: None,
    };
    save_plan(&dir.path("formula.arena.json"), &plan);
    assert_eq!(
        load_content(&dir.path("formula.arena.json"))
            .unwrap()
            .version
            .values
            .items[0]
            .damage,
        27
    );
}

#[test]
fn cancellation_between_real_tanu_tables_precedes_later_game_validation() {
    let dir = Directory::new();
    let mut document = TanuDocument::read(include_bytes!("../content/arena.tmd")).unwrap();
    let mut sources = document.sources().unwrap();
    let DataSourceDefinition::FormulaTable { rows, columns, .. } =
        sources.sources.get_mut("items").unwrap()
    else {
        panic!("managed items")
    };
    let damage = columns
        .iter()
        .position(|column| column.name == "damage")
        .unwrap();
    rows[0].cells[damage].content = FormulaTableCellContent::Formula {
        expression: "0 - 1".into(),
    };
    document.set_sources(sources).unwrap();
    std::fs::write(dir.path("later-invalid.tmd"), document.bytes().unwrap()).unwrap();
    let plan = SourcePlan {
        version: 1,
        base: SourceReference {
            format: SourceFormat::Tmd,
            path: "later-invalid.tmd".into(),
        },
        difficulty: None,
        event: None,
        debug: None,
    };
    save_plan(&dir.path("cancel.arena.json"), &plan);
    for path in [dir.path("later-invalid.tmd"), dir.path("cancel.arena.json")] {
        let unblocked = format!("{:#}", load_content(&path).unwrap_err());
        assert!(unblocked.contains("items"), "{unblocked}");
        let checks = std::sync::Arc::new(AtomicU64::new(0));
        let callback = checks.clone();
        let cancelled = format!(
            "{:#}",
            load_content_with_cancel(&path, move || callback.fetch_add(1, Ordering::Relaxed) >= 5)
                .unwrap_err()
        );
        assert_eq!(checks.load(Ordering::Relaxed), 6);
        assert!(cancelled.contains("cancelled"), "{cancelled}");
        assert!(
            !cancelled.contains("evaluate Tanu table items"),
            "{cancelled}"
        );
    }
}

fn semantic_outputs(bundles: &[OscBundle]) -> Vec<OscBundle> {
    let mut result = bundles.to_vec();
    for bundle in &mut result {
        for message in &mut bundle.messages {
            if message.address == "/game/arena/run" || message.address == "/ui/arena/content" {
                let OscArg::Str(encoded) = &mut message.args[0] else {
                    panic!("run JSON")
                };
                let mut run: Value = serde_json::from_str(encoded).unwrap();
                // Evaluator/source provenance intentionally differs by reader. Every
                // gameplay value, tick, receipt and other ordered event stays exact.
                for field in if message.address == "/game/arena/run" {
                    &["content"][..]
                } else {
                    &["active", "pending"][..]
                } {
                    if !run[field].is_null() {
                        let content = &run[field];
                        run[field] = json!({"hash":content["hash"],"values":content["values"]});
                    }
                }
                *encoded = serde_json::to_string(&run).unwrap();
            }
        }
    }
    result
}

#[test]
fn sqlite_full_stock_matches_tmd_gameplay_and_replays_after_source_deletion() {
    let dir = Directory::new();
    let path = dir.path("base.sqlite");
    write_sqlite(&path, &ArenaConfig::default().to_tables().unwrap()).unwrap();
    let version = load_content(&path).unwrap().version;
    let mut sqlite = build_demo_runtime().unwrap();
    arena::install_with_content(&mut sqlite, version.clone()).unwrap();
    let mut recorder = Recorder::new(&sqlite).unwrap();
    let counts = support::replay_reference_observing_ticks(
        "stock-eleven-death-retry",
        5528,
        |_| {},
        |reference, outputs| {
            for input in reference.committed_input_records() {
                assert_eq!(
                    sqlite
                        .try_enqueue_input(input.bundle, input.metadata)
                        .unwrap(),
                    input.sequence
                );
            }
            sqlite.tick_once().unwrap();
            let actual = sqlite.drain_output_buffer();
            assert_eq!(support::projection(&sqlite), support::projection(reference));
            assert_eq!(semantic_outputs(&actual), semantic_outputs(outputs));
            recorder.capture(&sqlite, &actual).unwrap();
        },
    );
    assert_eq!(counts, (550, 53));
    let bytes = recorder.encode().unwrap();
    std::fs::remove_file(&path).unwrap();
    let session = Session::decode(&bytes).unwrap();
    assert_eq!(session.manifest().initial_content, version);
    assert_eq!(
        session.verify().unwrap().state,
        support::projection(&sqlite)
    );
    let mut seek = session.runtime().unwrap();
    for _ in 0..5527 {
        session.tick(&mut seek).unwrap();
    }
    assert_eq!(support::projection(&seek)["inventory"]["health"], 0);
    session.tick(&mut seek).unwrap();
    assert_eq!(support::projection(&seek), support::projection(&sqlite));
}

#[test]
fn layered_recording_keeps_detached_origins_after_all_sources_are_removed() {
    let dir = Directory::new();
    create_source_plan(&dir.path("run.arena.json")).unwrap();
    let loaded = load_content(&dir.path("run.arena.json")).unwrap();
    let mut runtime = build_demo_runtime().unwrap();
    arena::install_with_content(&mut runtime, loaded.version.clone()).unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    support::send(&mut runtime, "layered", 1, "/input/arena/start", vec![]);
    for _ in 0..4 {
        runtime.tick_once().unwrap();
        let output = runtime.drain_output_buffer();
        recorder.capture(&runtime, &output).unwrap();
    }
    let bytes = recorder.encode().unwrap();
    for location in loaded.sources {
        std::fs::remove_file(location.path).unwrap();
    }
    std::fs::remove_file(dir.path("run.arena.json")).unwrap();
    let session = Session::decode(&bytes).unwrap();
    assert_eq!(session.manifest().initial_content, loaded.version);
    assert_eq!(
        session.verify().unwrap().state["inventory"]["equipment"][0]["damage"],
        32
    );
    assert_eq!(
        content_origins(&session.manifest().initial_content).unwrap()["/items/starter/damage"],
        Layer::Debug
    );
}
