# Arena content sources and layers

Stage 12 of [the Arena roadmap](https://github.com/Nagitch/kitu-logic-processor/issues/129),
tracked by [Issue 152](https://github.com/Nagitch/kitu-logic-processor/issues/152).

## Ownership and execution

`kitu-data-sqlite` owns read-only SQLite snapshot access, schema checks, native
scalar decoding, ordering and resource limits. `apps/demo-game` owns Arena table
names, identities, game constraints, sparse patches and detached content versions.
`kitu-data-tmd` uses the pinned public Tanu API for document and Formula evaluation.
Neither the host nor SQLite adapter interprets Formula expressions independently.

The shared server/native host reads authoring files outside the simulation lock
only on explicit validation. The initial Runtime configuration remains embedded
or caller-supplied detached content. Validation produces one complete candidate;
staging submits its exact reviewed values through the ordinary management queue.
Only the next start/retry adopts it. Invalid evaluation retains current and pending
versions. The owner continues management ticks while gameplay is paused.

## Source selection

`KITU_ARENA_CONTENT` selects a `.tmd`, `.sqlite` or `.arena.json` source. If unset,
the server accepts the legacy `KITU_ARENA_TMD` variable, then the bundled authoring
path. Native `contentPath` and Unity `--arena-content` select the same formats.
No HTTP endpoint accepts a filesystem path or SQL statement.

```json
{
  "version": 1,
  "base": {"format": "sqlite", "path": "base.sqlite"},
  "difficulty": {"format": "tmd", "path": "difficulty.tmd"},
  "event": {"format": "sqlite", "path": "event.sqlite"},
  "debug": {"format": "tmd", "path": "debug.tmd"}
}
```

Base is required; other slots are optional. Application order is always
`base → difficulty → event → debug`. Relative paths resolve beside the plan;
absolute paths are accepted for local development. Unknown plan fields/formats,
empty paths and unsupported versions fail validation. Plans are limited to
128 KiB and each TMD source to 16 MiB. The entire serialized detached candidate
must fit the Runtime's 128 KiB configuration boundary before it can be staged.

## Typed tables and sparse overrides

Both formats supply `items`, `enemies`, `difficulty` and `chests` to
`ArenaConfig::from_tables`. Complete bases must contain all required columns and
pass the existing reference-preserving types, numeric ranges and references.
Integral finite real values are accepted where the existing TMD contract accepts
an integer; strings are never coerced to numbers. NaN/infinity, blobs, invalid
UTF-8, null assignments and unknown fields are rejected.

Overrides have these identities:

| Table | Sparse rule |
|---|---|
| `items` | Existing `id` plus any supported non-identity columns |
| `enemies` | Existing integer `kind` plus any supported non-identity columns |
| `difficulty` | Exactly one row when present, containing supported difficulty fields |
| `chests` | Replace the complete ordered list with all chest columns |

Missing tables/columns retain earlier values. Duplicate identities, unknown
identities and item/enemy insertion/deletion through a patch are invalid. A new
complete base may define a different valid item catalog. Chest row order and
repeated entries remain significant. Each application produces a fully validated
configuration; a later layer cannot conceal an invalid earlier layer.

## SQLite schema and snapshot identity

Arena databases use `PRAGMA user_version = 1`. Tables use the exact Arena column
names plus a unique integer `ordinal`. `arena-content create-sqlite PATH` writes a
complete default database; `create-plan PATH.arena.json` writes a four-layer
editable example. These commands do not overwrite existing output files.

All queries are constructed from trusted table specifications. A single read-only
transaction covers schema inspection and every table, including WAL-backed data.
Rows are ordered by the declared integer key; duplicate keys or wrong key types
fail instead of producing unstable order. Unknown tables/columns and unsupported
schema objects are rejected. An absent optional table differs from an empty table.

The source digest hashes the normalized typed snapshot actually queried,
including ordering keys and schema version. Hashing only the main `.sqlite` file
would miss committed changes still in the WAL. Paths, physical database layout
and SQLite connection objects never enter detached game data.

The SQLite adapter limits table/column/row counts, cell/value bytes, total bytes,
VM work and lock waits. It checks cancellation between reads and in SQLite's
progress callback, including work before the first output row. Host teardown can
cancel evaluation without admitting a partially loaded candidate. See the
[adapter documentation](../../crates/kitu-data-sqlite/README.md) for exact defaults.
Tanu cancellation is checked around public document/table calls and between
tables. A single public Tanu read/evaluation call is indivisible, including its
eager document validation; cancellation does not interrupt it internally.
Each file supplies a consistent snapshot; separate layer files do not share a
cross-file transaction. The reviewed ordered digests identify exactly the loaded
combination. Operators validate after saving their intended set of edits.

## Detached versions and compatibility

`ContentVersion.hash` continues to identify evaluated game values. A legacy
single-TMD version retains its original JSON fields and `tanuRevision`. A SQLite
or layered version omits that global Tanu field and carries `provenance`:

- Encoding version 1 and sources in fixed layer order.
- Each source's format, Arena schema version, actual evaluator and source digest.
- A winning layer for every semantic field, using RFC 6901 escaped paths such as
  `/items/starter/damage`. The ordered chest list is represented by `/chests`.

TMD source digests cover the raw document. SQLite evaluators are identified as
`arena-sqlite-scalars-v1`; TMD evaluators use revision
`194358e8791f1391492abcb60d8cfcc37bbb383a`. The layered `sourceSha256` hashes the
ordered detached provenance. A source-only edit changes the reviewed token even
if the game-value hash remains equal. Both hashes must match the latest candidate
when staging; stale requests return HTTP 409.

Run manifests and TSQ1 recordings retain complete evaluated values and provenance.
Replay reuses the normal input queue/tick implementation without reading current
authoring files. Source edits, relocation or deletion do not alter saved runs.
Execution-build, contract and tick-rate checks remain enforced; this feature does
not make recordings from different execution builds silently interchangeable.

## Operator inspection

`GET /arena/content` preserves the existing Runtime/candidate/diagnostic fields
and adds:

| Field | Meaning |
|---|---|
| `sources` | Latest validated candidate's layer, format, resolved authoring path, source digest, evaluator and schema |
| `differences.active` / `.pending` | Candidate value differences against the selected version; `null` if unavailable, `[]` if equal |
| `origins.candidate` / `.active` / `.pending` | Semantic field paths mapped to winning layers; legacy TMD fields originate at base |

A difference contains `path`, `before`, `after` and `winningLayer`. Missing fields
after replacement of a complete base use JSON null only in this diagnostic DTO;
null assignments in authoring remain invalid. Candidate diagnostics and source
paths are host metadata, outside deterministic gameplay state.

Admin's **Game Parameters** page exposes these fields, including searchable
origins and current/pending/candidate selection. **Reload and validate** evaluates;
**Apply to next run** stages. On failure, obsolete candidate sources/differences
are cleared while active/pending values and their provenance remain inspectable.
CLI and browser Shell's existing content commands call these same host endpoints.

## Validation

The stage's integration tests exercise real SQLite and public Tanu Formula APIs,
four-layer precedence, strict invalid patches, escaped IDs, chest order, WAL
source digests, detached replay after source removal and stale review tokens.
Default SQLite values are compared through the complete 5,528-tick stock input
trace against TMD values. Native HTTP tests validate/stage the same mixed sources,
retain last-valid content on failure and replay with deleted authoring files.
The frozen Unity-only inputs, expected outcomes and comparison tolerances remain
unchanged.
