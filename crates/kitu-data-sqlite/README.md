# kitu-data-sqlite

Read-only, bounded SQLite snapshots for Kitu applications. This is a real SQLite
boundary using the locked `rusqlite` dependency and bundled SQLite, replacing the
early in-memory `SqliteStore` fixture.

`read_snapshot(path, table_specs, options, cancelled)` opens an existing regular
file read-only and reads schema, `user_version` and all requested tables in one
transaction. Returned `Snapshot`/`TableSnapshot` values own their data; no
connection or filesystem access is needed after loading. Applications own their
schema, authoring/migrations, domain validation and next-run activation.

## Table and scalar contract

- Specifications name allowed and required columns. Present columns follow the
  requested order. Missing optional tables are omitted; present empty tables and
  SQL NULL remain explicit. This supports sparse application-owned overrides.
- Every table has an explicit, non-null INTEGER ordering tuple. Duplicate tuples
  are rejected. Ordering column names and key values are retained in the snapshot
  even when excluded from its data columns.
- INTEGER retains the complete signed 64-bit value; REAL must be finite; TEXT
  must be valid UTF-8; NULL is preserved. BLOB is rejected. The reader performs no
  Rust string/number conversion. SQLite may already have applied column affinity
  when the authoring tool inserted a value; the reader sees its stored type.
- Unknown columns and requested views/virtual/generated tables are rejected.
  Unknown user tables are rejected by default. Internal `sqlite_*` tables,
  indexes and triggers are not returned; no write SQL is executed.
- Identifiers are bounded ASCII names, validated before SQL construction. No raw
  SQL, connection or schema mutation interface is exposed to network clients.

The generic types implement `Serialize`. Applications should hash the ordered
snapshot with their own versioned canonical serializer. It includes every queried
data value and ordering key. **Do not hash only the main `.sqlite` file**:
committed WAL changes can alter loaded data while that file remains unchanged.
This digest describes the queried snapshot, not unrelated database contents.

## Bounds and cancellation

`ReadOptions::default()` permits 64 tables, 128 columns including ordering keys,
100,000 rows per table, 1,000,000 total cells, 1 MiB per scalar, and 16 MiB total
scalar/identifier bytes. Ordering-key cells also consume the cell/byte budgets.
Applications can lower these limits to their own schema. The row count is checked
using `LIMIT maximum+1`; it never silently truncates a successful snapshot.

The cancellation callback runs between reads and from the SQLite progress hook,
which also enforces a VM budget (10,000,000 reported operations by default).
Expensive sorting can be interrupted before a first row is returned. Progress
checks occur every at most 1,000 operations; short statements can finish before a
progress callback. SQLite also has per-row, SQL-size, column and worker limits.
No partial snapshot is returned after cancellation or a bound/type/schema failure.
Callback panics are contained and reported without crossing the SQLite C ABI.

The default busy timeout is 100 ms and cannot exceed five seconds. Cancellation
is checked again after a lock wait; it does not interrupt an operating-system
file read. The caller must supply a prompt, thread-safe cancellation callback and
run this blocking reader outside the simulation tick/lock. The function owns all
work synchronously and returns only after the connection has been dropped.

## Verification

Crate tests use real temporary databases and cover native scalar fidelity,
deterministic ordering, sparse/empty data, source serialization, rejected
schema/types, materialization limits, missing/corrupt files, bounded lock waits,
and cancellation/VM limits during sorting. WAL tests verify both a main-file-
unchanged update and concurrent atomic schema/multiple-table changes: a load sees
one consistent old snapshot, and the subsequent load sees the committed update.

## Publish readiness
- Status: internal-only (`publish = false`), but crates.io-facing metadata and README are prepared.
- Execute the gate before changing publication flags:
  - `cargo fmt --check`
  - `cargo clippy --all-targets --all-features -- -D warnings`
  - `cargo test`
  - `cargo doc --no-deps`
  - `cargo publish --dry-run`

## Related docs
- Workspace crate overview: `doc/crates-overview.md`
