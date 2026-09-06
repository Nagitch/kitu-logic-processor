# Arena boss scripts

Stage 13 connects real Rhai execution to the same Arena application used by the
server, replay and native library. `kitu-scripting-rhai` owns the restricted
execution policy; Arena owns copied context, allowed actions and run activation.
The frozen Unity-only source and expected gameplay remain unchanged.

## Authoring and activation

The reference source is `apps/demo-game/content/boss.rhai`. Set
`KITU_ARENA_SCRIPT=/absolute/path/boss.rhai` for the server's editable source.
Without that option, validation uses the bundled reference source. The native
bridge seeds `boss.rhai` in its storage directory only when absent; an explicit
absolute `scriptPath` selects another file. Unity accepts `--arena-script` with
that absolute path. Authoring paths never replace a factory-supplied Runtime.

Edit the source in an editor, then use Admin **Game Scripts → Reload and validate**
and **Apply to next run**, or the shared terminal/browser Shell commands:

```sh
kitu-cli --endpoint http://127.0.0.1:8787 inspect script
kitu-cli --endpoint http://127.0.0.1:8787 script validate
kitu-cli --endpoint http://127.0.0.1:8787 script stage <candidate-hash>
```

Changing the reference telegraph duration from `0.8` to `1.6` seconds is a useful
first experiment. Validation compiles and probes the source outside the simulation
lock. The candidate hash binds source bytes, Rhai version, host policy and the boss
contract. Stale tokens and invalid candidates are refused. A source-only comment
edit changes the token too.

Staging uses `/input/arena/script` with one JSON `ScriptVersion` string and the
reserved `host:arena-script-admin` producer for Admin/Shell. Native detached
staging and the public Rust helper retain `host:arena-script`; caller-chosen IDs
cannot exhaust the operator catalog's separate ID space. Native and network
inputs cannot impersonate the operator producer. Saved replays accept both
identities unchanged. Ordinary ordered admission, message IDs,
receipts and duplicate detection apply. Pending source becomes active only at
start/retry. Neither validation nor staging alters the active boss. Invalid edits
clear the authoring candidate but retain valid pending and active versions.

## Boss contract 1

The entry point is `fn boss(input)`. Each invocation receives a fresh copied map:
Contexts are captured before that tick's gameplay step, including before player
attacks and damage; all boss requests must succeed before that step is committed.

| Field | Type | Meaning |
|---|---|---|
| `phase` | integer | 0 pursuit, 1 telegraph, 2 recovery |
| `hp`, `maxHp` | integer | Current boss health and maximum health |
| `floor` | integer | Current one-based floor |
| `timerExpired` | boolean | Rust f32 timer after this update is at most 0.00001 seconds |

Return exactly `{ action, duration }` using a Rhai map `#{...}`. Duration is seconds.

| Action | Valid context | Effect |
|---|---|---|
| `pursue` | phase 0, timer not expired | Existing movement/melee; duration 0 |
| `telegraph` | phase 0, expired | Enter phase 1 for `(0,60]` seconds; suppress movement/melee |
| `burst` | phase 1, expired | Existing eight radial shots, then phase 2 for `(0,60]` seconds |
| `recover` | phase 2, expired | Enter phase 0 for `(0,60]` seconds; pursuit resumes next update |
| `wait` | phase 1 or phase 2 | Remain in phase; duration 0 |

All requests are validated before gameplay effects. Rust retains f32 subtraction,
trigonometry, projectile order/IDs, damage, movement and death/clear precedence.
The default script preserves reference durations 0.8/1.0/3.0 seconds. It receives no
ECS object, host handle, I/O capability or direct mutation callback.

## Diagnostics and execution policy

The shared host starts from Rhai's raw engine with selected pure packages and no
module resolver. It excludes imports, eval, time, random, print/debug
and host effects. Numeric Cargo features are not changed, preserving Tanu's unified
Rhai semantics. See the [crate policy and exact limits](../../crates/kitu-scripting-rhai/README.md).
The existing Tanu `no_module`/`no_closure` feature combination is supported without
changing Tanu's engine settings.
Source, JSON data, expressions, calls and executed operations have explicit bounds;
this is an in-process scripting policy rather than an operating-system sandbox.

Validation probes each phase and expired/non-expired timer with low/full health.
A probe cannot prove every conditional path. A late fault records tick, boss ID,
script hash and a bounded structured diagnostic, pauses the game and clears held
controls before committing any gameplay step. The consumed management tick is not
retried. Stage corrected rules, return to menu and start a new run to recover;
resume cannot bypass the fault. Diagnostics remain inspectable while paused.

GET `/arena/script` and POST `/arena/script/validate` return authoring path,
read-only mode, runtime active/pending versions and fault, candidate and validation
diagnostics. POST `/arena/script/stage` requires `{ "hash": "..." }` and returns
queue sequence/hash. CLI staging waits for its ordinary applied receipt. Admin
and Shell inspect the observed replay version; staging is refused during replay.

## Detached versions and replay

`ScriptVersion` contains `hash`, `sourceSha256`, `source`, `contractVersion`,
`policyVersion` and `rhaiVersion`. Every run-start event includes its complete
active version. Replay manifest 2 requires `initialScript`; later staged versions
travel through recorded normal input admission. No replay reads authoring files.
Changing or deleting those files leaves previously recorded runs reproducible with
their saved sources under the same execution version.

The execution fingerprint includes the scripting crate, its manifest, lockfile
and bundled reference script. A changed executable/policy is not silently treated
as compatible; recordings from earlier execution versions require their archived
executable. Native configuration may contain detached `script` alongside detached
`content`. Empty native configuration uses bundled defaults and starts no listener
or authoring I/O.

Authoring catalogs retain compiled candidates and playback preparation retains
initial/staged programs before acquiring the host clock lock. Queue admission
pins up to 64 distinct programs per pending batch; it never recompiles from a tick.
A recording permits at most 64 distinct script versions including its initial
version. Capture and loading enforce the same bound; exceeding capture capacity
reports a recording diagnostic and retains the preceding valid recording.
