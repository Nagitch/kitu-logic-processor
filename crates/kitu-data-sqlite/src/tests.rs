use super::*;
use rusqlite::Connection;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn database(sql: &str) -> (tempfile::TempDir, Connection) {
    let directory = tempfile::tempdir().unwrap();
    let connection = Connection::open(directory.path().join("data.sqlite")).unwrap();
    connection.execute_batch("PRAGMA user_version=1;").unwrap();
    connection.execute_batch(sql).unwrap();
    (directory, connection)
}

fn spec(name: &str, columns: &[&str]) -> TableSpec {
    let columns = columns.iter().map(|s| (*s).into()).collect::<Vec<_>>();
    TableSpec {
        name: name.into(),
        required_columns: columns.clone(),
        columns,
        order_by: vec!["ordinal".into()],
        optional: false,
    }
}

fn read(
    directory: &tempfile::TempDir,
    tables: &[TableSpec],
    options: &ReadOptions,
) -> Result<Snapshot> {
    read_snapshot(
        &directory.path().join("data.sqlite"),
        tables,
        options,
        || false,
    )
}

#[test]
fn native_scalars_ordering_and_serialized_source_provenance_are_detached() {
    let (directory, connection) = database(
        "CREATE TABLE values_table(ordinal INTEGER, minor INTEGER, i, r, t, n);
        INSERT INTO values_table VALUES(2, 0, -9223372036854775808, 1.5, '後', NULL);
        INSERT INTO values_table VALUES(1, 9, 9223372036854775807, 2.0, 'first', NULL);",
    );
    let mut table = spec("values_table", &["t", "i", "r", "n"]);
    table.order_by.push("minor".into());
    let snapshot = read(
        &directory,
        std::slice::from_ref(&table),
        &ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.tables[0].columns, ["t", "i", "r", "n"]);
    assert_eq!(snapshot.tables[0].order_keys, [vec![1, 9], vec![2, 0]]);
    assert_eq!(
        snapshot.tables[0].rows[0],
        [
            Scalar::Text("first".into()),
            Scalar::Integer(i64::MAX),
            Scalar::Real(2.0),
            Scalar::Null
        ]
    );
    assert_eq!(snapshot.tables[0].rows[1][1], Scalar::Integer(i64::MIN));
    let before = serde_json::to_vec(&snapshot).unwrap();
    connection
        .execute("UPDATE values_table SET ordinal=3 WHERE ordinal=2", [])
        .unwrap();
    let after = read(&directory, &[table], &ReadOptions::default()).unwrap();
    assert_eq!(snapshot.tables[0].rows, after.tables[0].rows);
    assert_ne!(
        before,
        serde_json::to_vec(&after).unwrap(),
        "changed explicit ordering keys must affect source provenance"
    );
    drop(connection);
    directory.close().unwrap();
    assert_eq!(serde_json::to_vec(&snapshot).unwrap(), before);
}

#[test]
fn sparse_columns_absent_tables_empty_tables_and_null_are_distinct() {
    let (directory, _) = database(
        "CREATE TABLE items(ordinal INTEGER, id TEXT, damage);
        INSERT INTO items VALUES(0, 'starter', NULL);
        CREATE TABLE chests(ordinal INTEGER, phase TEXT, itemId TEXT);",
    );
    let mut items = spec("items", &["id", "damage", "interval"]);
    items.required_columns = vec!["id".into()];
    let mut absent = spec("enemies", &["kind", "health"]);
    absent.optional = true;
    let snapshot = read(
        &directory,
        &[items, absent, spec("chests", &["phase", "itemId"])],
        &ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(snapshot.tables.len(), 2);
    assert_eq!(snapshot.tables[0].columns, ["id", "damage"]);
    assert_eq!(
        snapshot.tables[0].rows[0],
        [Scalar::Text("starter".into()), Scalar::Null]
    );
    assert!(snapshot.tables[1].rows.is_empty());
}

#[test]
fn rejects_bad_storage_types_and_ambiguous_ordering_without_coercion() {
    for (literal, expected) in [
        ("x'0102'", "BLOB"),
        ("CAST(x'ff' AS TEXT)", "UTF-8"),
        ("1e999", "non-finite"),
    ] {
        let (directory, _) = database(&format!(
            "CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, {literal});"
        ));
        let error = read(
            &directory,
            &[spec("items", &["value"])],
            &ReadOptions::default(),
        )
        .unwrap_err();
        assert!(matches!(error, SqliteError::InvalidValue { .. }));
        assert!(error.to_string().contains(expected));
    }
    for key in ["NULL", "'one'", "1.5"] {
        let (directory, _) = database(&format!(
            "CREATE TABLE items(ordinal, value); INSERT INTO items VALUES({key}, 1);"
        ));
        assert!(read(
            &directory,
            &[spec("items", &["value"])],
            &ReadOptions::default()
        )
        .unwrap_err()
        .to_string()
        .contains("non-null INTEGER"));
    }
    let (directory, _) = database(
        "CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, 1),(0, 2);",
    );
    assert!(read(
        &directory,
        &[spec("items", &["value"])],
        &ReadOptions::default()
    )
    .unwrap_err()
    .to_string()
    .contains("duplicate ordering"));
}

#[test]
fn schema_contract_rejects_missing_unknown_generated_and_view_sources() {
    let (directory, connection) = database(
        "CREATE TABLE items(ordinal INTEGER, value); CREATE TABLE extra(ordinal INTEGER, value);",
    );
    let table = spec("items", &["value"]);
    assert!(matches!(
        read(
            &directory,
            std::slice::from_ref(&table),
            &ReadOptions::default()
        ),
        Err(SqliteError::InvalidSchema(_))
    ));
    let relaxed = ReadOptions {
        deny_unknown_tables: false,
        ..ReadOptions::default()
    };
    read(&directory, std::slice::from_ref(&table), &relaxed).unwrap();
    let wrong_version = ReadOptions {
        expected_schema_version: Some(2),
        ..relaxed.clone()
    };
    assert!(
        read(&directory, std::slice::from_ref(&table), &wrong_version)
            .unwrap_err()
            .to_string()
            .contains("user_version")
    );
    assert!(read(&directory, &[spec("missing", &["value"])], &relaxed)
        .unwrap_err()
        .to_string()
        .contains("missing table"));
    assert!(read(&directory, &[spec("items", &["missing"])], &relaxed).is_err());
    connection
        .execute_batch("ALTER TABLE items ADD COLUMN unknown INTEGER;")
        .unwrap();
    assert!(read(&directory, &[table], &relaxed)
        .unwrap_err()
        .to_string()
        .contains("unexpected/generated column"));
    connection.execute_batch("CREATE TABLE generated(ordinal INTEGER, value INTEGER GENERATED ALWAYS AS (ordinal+1)); CREATE VIEW viewed AS SELECT * FROM extra;").unwrap();
    assert!(read(&directory, &[spec("generated", &["value"])], &relaxed)
        .unwrap_err()
        .to_string()
        .contains("generated column"));
    assert!(read(&directory, &[spec("viewed", &["value"])], &relaxed)
        .unwrap_err()
        .to_string()
        .contains("unexpected view"));
}

#[test]
fn table_discovery_cannot_be_shadowed_by_an_allowed_database_table() {
    let (directory, connection) = database(
        "CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, 42);
        CREATE TABLE pragma_table_list(ordinal INTEGER, schema TEXT, name TEXT, type TEXT);
        INSERT INTO pragma_table_list VALUES(0, 'main', 'items', 'table'),
            (1, 'main', 'pragma_table_list', 'table');",
    );
    let tables = [
        spec("items", &["value"]),
        spec("pragma_table_list", &["schema", "name", "type"]),
    ];
    let snapshot = read(&directory, &tables, &ReadOptions::default()).unwrap();
    assert_eq!(snapshot.tables.len(), 2);
    assert_eq!(snapshot.tables[0].rows[0], [Scalar::Integer(42)]);
    assert_eq!(snapshot.tables[1].rows.len(), 2);

    // This table is deliberately absent from the ordinary pragma_table_list
    // data. Schema validation must use SQLite's actual table inventory.
    connection
        .execute_batch("CREATE TABLE extra(ordinal INTEGER, value);")
        .unwrap();
    let error = read(&directory, &tables, &ReadOptions::default()).unwrap_err();
    assert!(matches!(error, SqliteError::InvalidSchema(_)));
    assert!(error.to_string().contains("unexpected table: extra"));
    let relaxed = ReadOptions {
        deny_unknown_tables: false,
        ..ReadOptions::default()
    };
    assert_eq!(read(&directory, &tables, &relaxed).unwrap(), snapshot);
}

#[test]
fn column_discovery_cannot_be_shadowed_by_an_allowed_database_table() {
    let (directory, connection) = database(
        "CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, 42);
        CREATE TABLE pragma_table_xinfo(ordinal INTEGER, name TEXT, hidden INTEGER);
        INSERT INTO pragma_table_xinfo VALUES(0, 'value', 0);",
    );
    let tables = [
        spec("items", &["value"]),
        spec("pragma_table_xinfo", &["name", "hidden"]),
    ];
    let snapshot = read(&directory, &tables, &ReadOptions::default()).unwrap();
    assert_eq!(snapshot.tables[0].rows[0], [Scalar::Integer(42)]);
    assert_eq!(
        snapshot.tables[1].rows[0],
        [Scalar::Text("value".into()), Scalar::Integer(0)]
    );

    connection
        .execute_batch("ALTER TABLE items ADD COLUMN unexpected INTEGER;")
        .unwrap();
    let error = read(&directory, &tables, &ReadOptions::default()).unwrap_err();
    assert!(matches!(error, SqliteError::InvalidSchema(_)));
    assert!(error
        .to_string()
        .contains("unexpected/generated column: items/unexpected"));
}

#[test]
fn missing_files_are_not_created_and_reads_do_not_mutate_database_bytes() {
    let (directory, connection) =
        database("CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, 42);");
    let missing = directory.path().join("missing.sqlite");
    assert!(matches!(
        read_snapshot(
            &missing,
            &[spec("items", &["value"])],
            &ReadOptions::default(),
            || false
        ),
        Err(SqliteError::Io(_))
    ));
    assert!(!missing.exists());
    drop(connection);
    let path = directory.path().join("data.sqlite");
    let bytes = std::fs::read(&path).unwrap();
    read(
        &directory,
        &[spec("items", &["value"])],
        &ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    std::fs::write(&path, b"not a SQLite database").unwrap();
    assert!(matches!(
        read(
            &directory,
            &[spec("items", &["value"])],
            &ReadOptions::default()
        ),
        Err(SqliteError::Database(_))
    ));
}

#[test]
fn identifiers_and_materialization_limits_reject_before_partial_success() {
    let (directory, _) = database("CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, '123456789'),(1, 'other');");
    let table = spec("items", &["value"]);
    let mut bad = table.clone();
    bad.name = "items; DROP TABLE items".into();
    assert!(matches!(
        read(&directory, &[bad], &ReadOptions::default()),
        Err(SqliteError::InvalidSpec(_))
    ));
    assert!(matches!(
        read(
            &directory,
            &[table.clone(), table.clone()],
            &ReadOptions::default()
        ),
        Err(SqliteError::InvalidSpec(_))
    ));
    for options in [
        ReadOptions {
            max_rows_per_table: 1,
            ..ReadOptions::default()
        },
        ReadOptions {
            max_total_cells: 3,
            ..ReadOptions::default()
        },
        ReadOptions {
            max_value_bytes: 8,
            ..ReadOptions::default()
        },
        ReadOptions {
            max_total_bytes: 8,
            ..ReadOptions::default()
        },
    ] {
        assert!(matches!(
            read(&directory, std::slice::from_ref(&table), &options),
            Err(SqliteError::LimitExceeded(_))
        ));
    }
    let invalid = ReadOptions {
        busy_timeout: Duration::from_secs(6),
        ..ReadOptions::default()
    };
    assert!(matches!(
        read(&directory, &[table], &invalid),
        Err(SqliteError::InvalidSpec(_))
    ));
}

#[test]
fn committed_wal_values_change_snapshot_even_when_main_file_is_unchanged() {
    let (directory, connection) = database(
        "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
        CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, 10);",
    );
    let table = spec("items", &["value"]);
    let old = read(
        &directory,
        std::slice::from_ref(&table),
        &ReadOptions::default(),
    )
    .unwrap();
    let main_before = std::fs::read(directory.path().join("data.sqlite")).unwrap();
    connection.execute("UPDATE items SET value=20", []).unwrap();
    assert_eq!(
        std::fs::read(directory.path().join("data.sqlite")).unwrap(),
        main_before
    );
    let new = read(&directory, &[table], &ReadOptions::default()).unwrap();
    assert_eq!(old.tables[0].rows[0][0], Scalar::Integer(10));
    assert_eq!(new.tables[0].rows[0][0], Scalar::Integer(20));
    assert_ne!(
        serde_json::to_vec(&old).unwrap(),
        serde_json::to_vec(&new).unwrap()
    );
}

#[test]
fn schema_and_multiple_tables_share_one_snapshot_during_concurrent_wal_commit() {
    let (directory, writer) = database(
        "PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;
        CREATE TABLE first(ordinal INTEGER, value); INSERT INTO first VALUES(0, 10);
        CREATE TABLE second(ordinal INTEGER, value); INSERT INTO second VALUES(0, 10);",
    );
    let writer = std::sync::Mutex::new(writer);
    let calls = AtomicUsize::new(0);
    let tables = [spec("first", &["value"]), spec("second", &["value"])];
    let snapshot = read_snapshot(
        &directory.path().join("data.sqlite"),
        &tables,
        &ReadOptions::default(),
        move || {
            // The first two checks precede BEGIN; the next two occur while the
            // schema inventory is being consumed (well below one VM quantum).
            // Commit both schema and data before any application table is read.
            if calls.fetch_add(1, Ordering::Relaxed) == 3 {
                writer
                    .lock()
                    .unwrap()
                    .execute_batch(
                        "BEGIN;
                UPDATE first SET value=20; UPDATE second SET value=20;
                ALTER TABLE first ADD COLUMN extra INTEGER; COMMIT;",
                    )
                    .unwrap();
            }
            false
        },
    )
    .unwrap();
    assert_eq!(snapshot.tables[0].rows[0][0], Scalar::Integer(10));
    assert_eq!(snapshot.tables[1].rows[0][0], Scalar::Integer(10));
    assert!(read(&directory, &tables, &ReadOptions::default())
        .unwrap_err()
        .to_string()
        .contains("extra"));
    let mut updated = tables;
    updated[0].columns.push("extra".into());
    let new = read(&directory, &updated, &ReadOptions::default()).unwrap();
    assert_eq!(new.tables[0].rows[0][0], Scalar::Integer(20));
    assert_eq!(new.tables[1].rows[0][0], Scalar::Integer(20));
}

#[test]
fn cancellation_and_vm_budget_interrupt_sorting_before_first_row() {
    let (directory, _) = database(
        "CREATE TABLE items(ordinal INTEGER, value);
        WITH RECURSIVE n(x) AS (VALUES(25000) UNION ALL SELECT x-1 FROM n WHERE x>0)
        INSERT INTO items SELECT x,x FROM n;",
    );
    let table = spec("items", &["value"]);
    let zero_rows = ReadOptions {
        max_rows_per_table: 0,
        ..ReadOptions::default()
    };
    let calls = Arc::new(AtomicUsize::new(0));
    let callback_calls = calls.clone();
    let cancelled = read_snapshot(
        &directory.path().join("data.sqlite"),
        std::slice::from_ref(&table),
        &zero_rows,
        move || callback_calls.fetch_add(1, Ordering::Relaxed) >= 30,
    );
    assert!(
        matches!(cancelled, Err(SqliteError::Cancelled)),
        "the VM must cancel before LIMIT 1 returns the first disallowed row"
    );
    assert_eq!(calls.load(Ordering::Relaxed), 31);
    let limited = ReadOptions {
        max_vm_steps: 4_000,
        ..zero_rows
    };
    assert!(matches!(
        read(&directory, std::slice::from_ref(&table), &limited),
        Err(SqliteError::LimitExceeded("SQLite VM operations"))
    ));
    assert!(matches!(
        read_snapshot(
            &directory.path().join("absent"),
            std::slice::from_ref(&table),
            &ReadOptions::default(),
            || true
        ),
        Err(SqliteError::Cancelled)
    ));
    assert!(matches!(
        read_snapshot(
            &directory.path().join("data.sqlite"),
            &[table],
            &ReadOptions::default(),
            || panic!("test callback")
        ),
        Err(SqliteError::CancellationCallbackPanicked)
    ));
}

#[test]
fn lock_wait_is_bounded() {
    let (directory, connection) = database("CREATE TABLE items(ordinal INTEGER, value); INSERT INTO items VALUES(0, 1); BEGIN EXCLUSIVE;");
    let options = ReadOptions {
        busy_timeout: Duration::from_millis(20),
        ..ReadOptions::default()
    };
    let started = std::time::Instant::now();
    let error = read(&directory, &[spec("items", &["value"])], &options).unwrap_err();
    assert!(matches!(error, SqliteError::Database(_)));
    assert!(started.elapsed() < Duration::from_secs(1));
    connection.execute_batch("ROLLBACK").unwrap();
    read(&directory, &[spec("items", &["value"])], &options).unwrap();
}
