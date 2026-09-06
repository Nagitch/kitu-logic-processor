use super::*;
use rusqlite::{
    config::DbConfig, limits::Limit, types::ValueRef, Connection, OpenFlags, Transaction,
};
use std::{
    collections::BTreeSet,
    panic::{catch_unwind, AssertUnwindSafe},
    sync::{
        atomic::{AtomicU64, AtomicU8, Ordering},
        Arc,
    },
};

struct Control {
    cancelled: Box<dyn Fn() -> bool + Send + Sync>,
    stopped: AtomicU8,
    steps: AtomicU64,
    max_steps: u64,
}

impl Control {
    fn reason(&self) -> Option<SqliteError> {
        match self.stopped.load(Ordering::Relaxed) {
            1 => Some(SqliteError::Cancelled),
            2 => Some(SqliteError::CancellationCallbackPanicked),
            3 => Some(SqliteError::LimitExceeded("SQLite VM operations")),
            _ => None,
        }
    }

    fn check(&self) -> Result<()> {
        if let Some(error) = self.reason() {
            return Err(error);
        }
        match catch_unwind(AssertUnwindSafe(|| (self.cancelled)())) {
            Ok(false) => return Ok(()),
            Ok(true) => self.stopped.store(1, Ordering::Relaxed),
            Err(payload) => {
                // A user-defined panic payload can itself panic during Drop.
                std::mem::forget(payload);
                self.stopped.store(2, Ordering::Relaxed);
            }
        }
        Err(self.reason().expect("cancellation reason was set"))
    }

    fn progress(&self, quantum: u64) -> bool {
        if self.check().is_err() {
            return true;
        }
        if self.steps.fetch_add(quantum, Ordering::Relaxed) + quantum >= self.max_steps {
            self.stopped.store(3, Ordering::Relaxed);
            return true;
        }
        false
    }
}

struct Budget<'a> {
    options: &'a ReadOptions,
    cells: usize,
    bytes: usize,
}

impl Budget<'_> {
    fn bytes(&mut self, length: usize) -> Result<()> {
        self.bytes = self
            .bytes
            .checked_add(length)
            .filter(|n| *n <= self.options.max_total_bytes)
            .ok_or(SqliteError::LimitExceeded("total bytes"))?;
        Ok(())
    }
    fn cell(&mut self, length: usize) -> Result<()> {
        if length > self.options.max_value_bytes {
            return Err(SqliteError::LimitExceeded("scalar bytes"));
        }
        self.cells = self
            .cells
            .checked_add(1)
            .filter(|n| *n <= self.options.max_total_cells)
            .ok_or(SqliteError::LimitExceeded("total cells"))?;
        self.bytes(length)
    }
}

fn identifier(value: &str) -> bool {
    value.len() <= 128
        && value
            .as_bytes()
            .first()
            .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

fn unique_names(names: &[String]) -> bool {
    names.iter().all(|s| identifier(s))
        && names.iter().collect::<BTreeSet<_>>().len() == names.len()
}

fn validate(specs: &[TableSpec], options: &ReadOptions) -> Result<()> {
    if specs.is_empty()
        || specs.len() > options.max_tables
        || options.max_tables == 0
        || options.max_tables > 1024
        || options.max_columns == 0
        || options.max_columns > 1024
        || options.max_rows_per_table >= i64::MAX as usize
        || options.max_total_cells == 0
        || options.max_value_bytes == 0
        || options.max_total_bytes == 0
        || options.max_total_bytes > i32::MAX as usize
        || options.max_vm_steps == 0
        || options.max_vm_steps > 1_000_000_000
        || options.busy_timeout > Duration::from_secs(5)
    {
        return Err(SqliteError::InvalidSpec(
            "invalid read limits or table count".into(),
        ));
    }
    let mut tables = BTreeSet::new();
    for spec in specs {
        if !identifier(&spec.name)
            || spec.name.to_ascii_lowercase().starts_with("sqlite_")
            || !tables.insert(spec.name.to_ascii_lowercase())
            || spec.columns.is_empty()
            || spec.columns.len() + spec.order_by.len() > options.max_columns
            || !unique_names(&spec.columns)
            || !unique_names(&spec.required_columns)
            || !unique_names(&spec.order_by)
            || spec.order_by.is_empty()
            || spec
                .required_columns
                .iter()
                .any(|s| !spec.columns.contains(s))
        {
            return Err(SqliteError::InvalidSpec(format!(
                "invalid table specification: {}",
                spec.name
            )));
        }
    }
    Ok(())
}

pub(super) fn read(
    path: &Path,
    specs: &[TableSpec],
    options: &ReadOptions,
    cancelled: impl Fn() -> bool + Send + Sync + 'static,
) -> Result<Snapshot> {
    validate(specs, options)?;
    let control = Arc::new(Control {
        cancelled: Box::new(cancelled),
        stopped: AtomicU8::new(0),
        steps: AtomicU64::new(0),
        max_steps: options.max_vm_steps,
    });
    control.check()?;
    if !std::fs::metadata(path)?.is_file() {
        return Err(SqliteError::InvalidSpec(
            "source must be an existing regular file".into(),
        ));
    }
    let mut connection = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    connection.busy_timeout(options.busy_timeout)?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_DEFENSIVE, true)?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_TRUSTED_SCHEMA, false)?;
    connection.set_db_config(DbConfig::SQLITE_DBCONFIG_DQS_DML, false)?;
    connection.set_limit(
        Limit::SQLITE_LIMIT_LENGTH,
        options.max_total_bytes.max(65_536) as i32,
    )?;
    connection.set_limit(Limit::SQLITE_LIMIT_SQL_LENGTH, 262_144)?;
    connection.set_limit(
        Limit::SQLITE_LIMIT_COLUMN,
        (options.max_columns * 2).max(32) as i32,
    )?;
    connection.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)?;
    connection.set_limit(Limit::SQLITE_LIMIT_WORKER_THREADS, 0)?;
    let hook = control.clone();
    let quantum = options.max_vm_steps.min(1000);
    connection.progress_handler(quantum as i32, Some(move || hook.progress(quantum)))?;
    let result = (|| {
        control.check()?;
        let transaction = connection.transaction()?;
        let snapshot = read_transaction(&transaction, specs, options, &control)?;
        control.check()?;
        transaction.commit()?;
        Ok(snapshot)
    })();
    // A cancellation request can arrive while SQLite is in its bounded busy
    // wait, where the progress handler does not run.
    if control.reason().is_none() {
        control.check()?;
    }
    match control.reason() {
        Some(error) => Err(error),
        None => result,
    }
}

fn read_transaction(
    transaction: &Transaction<'_>,
    specs: &[TableSpec],
    options: &ReadOptions,
    control: &Control,
) -> Result<Snapshot> {
    let version: i64 = transaction.query_row("PRAGMA main.user_version", [], |row| row.get(0))?;
    let schema_version = u32::try_from(version)
        .map_err(|_| SqliteError::InvalidSchema(format!("negative user_version: {version}")))?;
    if options
        .expected_schema_version
        .is_some_and(|expected| expected != schema_version)
    {
        return Err(SqliteError::InvalidSchema(format!(
            "user_version {schema_version}, expected {}",
            options.expected_schema_version.unwrap()
        )));
    }
    let mut budget = Budget {
        options,
        cells: 0,
        bytes: 0,
    };
    let mut present = BTreeSet::new();
    let mut query = transaction.prepare("SELECT name,type FROM pragma_table_list WHERE schema='main' AND name NOT LIKE 'sqlite_%' ORDER BY name LIMIT ?1")?;
    let mut rows = query.query([options.max_tables as i64 + 1])?;
    while let Some(row) = rows.next()? {
        control.check()?;
        let name = row
            .get_ref(0)?
            .as_str()
            .map_err(|_| SqliteError::InvalidSchema("non-UTF-8 table name".into()))?;
        if present.len() == options.max_tables {
            return Err(SqliteError::LimitExceeded("tables"));
        }
        let requested = specs.iter().any(|spec| spec.name == name);
        let kind = row
            .get_ref(1)?
            .as_str()
            .map_err(|_| SqliteError::InvalidSchema("invalid table kind".into()))?;
        if (options.deny_unknown_tables && !requested) || (requested && kind != "table") {
            return Err(SqliteError::InvalidSchema(format!(
                "unexpected {kind}: {name}"
            )));
        }
        if !identifier(name) {
            return Err(SqliteError::InvalidSchema(
                "unsupported table identifier".into(),
            ));
        }
        budget.bytes(name.len())?;
        present.insert(name.to_owned());
    }
    drop(rows);
    drop(query);
    let mut tables = Vec::new();
    for spec in specs {
        control.check()?;
        if !present.contains(&spec.name) {
            if spec.optional {
                continue;
            }
            return Err(SqliteError::InvalidSchema(format!(
                "missing table: {}",
                spec.name
            )));
        }
        tables.push(read_table(transaction, spec, control, &mut budget)?);
    }
    Ok(Snapshot {
        schema_version,
        tables,
    })
}

fn read_table(
    transaction: &Transaction<'_>,
    spec: &TableSpec,
    control: &Control,
    budget: &mut Budget<'_>,
) -> Result<TableSnapshot> {
    let mut present = BTreeSet::new();
    let mut query = transaction
        .prepare("SELECT name,hidden FROM pragma_table_xinfo(?1, 'main') ORDER BY cid LIMIT ?2")?;
    let mut rows = query.query(rusqlite::params![
        spec.name,
        budget.options.max_columns as i64 + 1
    ])?;
    while let Some(row) = rows.next()? {
        control.check()?;
        if present.len() == budget.options.max_columns {
            return Err(SqliteError::LimitExceeded("columns"));
        }
        let name = row
            .get_ref(0)?
            .as_str()
            .map_err(|_| SqliteError::InvalidSchema("non-UTF-8 column name".into()))?;
        if row.get::<_, i32>(1)? != 0
            || (!spec.columns.iter().any(|s| s == name) && !spec.order_by.iter().any(|s| s == name))
        {
            return Err(SqliteError::InvalidSchema(format!(
                "unexpected/generated column: {}/{name}",
                spec.name
            )));
        }
        budget.bytes(name.len())?;
        present.insert(name.to_owned());
    }
    for name in spec.required_columns.iter().chain(&spec.order_by) {
        if !present.contains(name) {
            return Err(SqliteError::InvalidSchema(format!(
                "missing column: {}/{name}",
                spec.name
            )));
        }
    }
    drop(rows);
    drop(query);
    let columns = spec
        .columns
        .iter()
        .filter(|name| present.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    let selected = columns
        .iter()
        .chain(&spec.order_by)
        .map(|s| format!("\"{s}\""))
        .collect::<Vec<_>>()
        .join(",");
    let ordered = spec
        .order_by
        .iter()
        .map(|s| format!("\"{s}\" ASC"))
        .collect::<Vec<_>>()
        .join(",");
    let sql = format!(
        "SELECT {selected} FROM main.\"{}\" ORDER BY {ordered} LIMIT ?1",
        spec.name
    );
    let mut query = transaction.prepare(&sql)?;
    let mut rows = query.query([budget.options.max_rows_per_table as i64 + 1])?;
    let mut table = TableSnapshot {
        name: spec.name.clone(),
        columns,
        rows: Vec::new(),
        order_by: spec.order_by.clone(),
        order_keys: Vec::new(),
    };
    while let Some(row) = rows.next()? {
        control.check()?;
        let index = table.rows.len();
        if index == budget.options.max_rows_per_table {
            return Err(SqliteError::LimitExceeded("rows per table"));
        }
        let mut key = Vec::with_capacity(spec.order_by.len());
        for (offset, name) in spec.order_by.iter().enumerate() {
            let ValueRef::Integer(value) = row.get_ref(table.columns.len() + offset)? else {
                return Err(invalid_value(
                    spec,
                    index,
                    name,
                    "ordering keys must be non-null INTEGER values",
                ));
            };
            budget.cell(8)?;
            key.push(value);
        }
        if table
            .order_keys
            .last()
            .is_some_and(|previous| *previous == key)
        {
            return Err(SqliteError::InvalidSchema(format!(
                "duplicate ordering tuple in {} at row {index}",
                spec.name
            )));
        }
        let mut values = Vec::with_capacity(table.columns.len());
        for (column, name) in table.columns.iter().enumerate() {
            values.push(scalar(row.get_ref(column)?, spec, index, name, budget)?);
        }
        table.rows.push(values);
        table.order_keys.push(key);
    }
    Ok(table)
}

fn invalid_value(spec: &TableSpec, row: usize, column: &str, reason: &'static str) -> SqliteError {
    SqliteError::InvalidValue {
        table: spec.name.clone(),
        row,
        column: column.into(),
        reason,
    }
}

fn scalar(
    value: ValueRef<'_>,
    spec: &TableSpec,
    row: usize,
    column: &str,
    budget: &mut Budget<'_>,
) -> Result<Scalar> {
    let length = match value {
        ValueRef::Null => 0,
        ValueRef::Integer(_) | ValueRef::Real(_) => 8,
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => bytes.len(),
    };
    budget.cell(length)?;
    Ok(match value {
        ValueRef::Null => Scalar::Null,
        ValueRef::Integer(value) => Scalar::Integer(value),
        ValueRef::Real(value) if value.is_finite() => Scalar::Real(value),
        ValueRef::Real(_) => return Err(invalid_value(spec, row, column, "non-finite REAL")),
        ValueRef::Text(bytes) => Scalar::Text(
            std::str::from_utf8(bytes)
                .map_err(|_| invalid_value(spec, row, column, "invalid UTF-8 TEXT"))?
                .to_owned(),
        ),
        ValueRef::Blob(_) => {
            return Err(invalid_value(
                spec,
                row,
                column,
                "BLOB is not a supported scalar",
            ))
        }
    })
}
