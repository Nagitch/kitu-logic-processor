# Live Kitu CLI and browser Shell

Stage 9 of [#129](https://github.com/Nagitch/kitu-logic-processor/issues/129)
connects `kitu-cli` and **Kitu general → Shell** to the running application host.
`kitu-shell` owns the shared catalog, argument validation, quoting, typed OSC
conversion, and request/result definitions. The browser sends its line to the
same Rust parser used by the CLI REPL. The server never starts an OS shell or
expands variables, substitutions, wildcards or scripts from command text.

## Start and operate

Run inside the Kitu Dev Container, with the demo host available:

```sh
cargo build -p kitu-demo-game -p kitu-cli --bins
cargo run -p kitu-demo-game --bin kitu-demo-game-admin-host
```

In another terminal:

```sh
target/debug/kitu-cli --endpoint http://127.0.0.1:8787 help
target/debug/kitu-cli app action list
target/debug/kitu-cli app action run arena.start
target/debug/kitu-cli app action run arena.pause
target/debug/kitu-cli inspect application
target/debug/kitu-cli app action run arena.resume
target/debug/kitu-cli scenario run preparation-smoke
target/debug/kitu-cli shell
```

`KITU_RUNTIME_URL` supplies the endpoint default. `--help` is available offline;
`help` fetches the connected host's definitions. JSON results go to stdout and
connection/request identities to stderr. Refusal or failure exits nonzero. The
REPL accepts one command per line, `exit`, and quoted strings; piped scripts keep
reading and return nonzero if any command failed. The client initially supports
HTTP development hosts; remote authentication/TLS operation is outside this plan.

The browser Shell shows the same definitions, recent results/refusals, complete
JSON and Up/Down history. Reconnect refreshes the host identity. Failed HTTP calls
retain the exact request for **Retry same request**; they are never silently
resent under a new ID.

## Commands and ownership

Use `help` for the authoritative catalog. The initial command families are:

| Command | Observable result |
| --- | --- |
| `inspect application/world/content/recording/replay` | State, recorded tick, settings or replay diagnostics from the active host |
| `osc send <address> [typed arguments]` | Normal input admission and applied outcome |
| `app action list/describe/run` | Runtime catalog and application-owned actions |
| `content validate` | Actual Tanu evaluation; invalid diagnostics produce a refusal |
| `content stage <hash> <source-sha256>` | Reviewed next-run candidate, with its applied receipt |
| `replay list/save/verify/load/play/pause/step/stop/seek/live` | Existing TSQ1 and playback operations; step returns after advancement |
| `scenario list/run` | Application-owned bounded action sequences on the live Runtime |

OSC types are explicit: `i:` i32, `h:` i64, `f:` finite f32, `s:` string, `b:`
`true`/`false`. For example, the legacy move delta retains its existing meaning:

```sh
kitu-cli osc send /input/move 's:example player' f:1 f:0
kitu-cli app action run spawn-object kind=marker x=1 y=2 z=3
```

Arena commands use the normal versioned metadata and application queue. Generic
World actions are also applied at the Runtime tick, with ordered outcome receipts
at `/ui/kitu/command` (`sequence`, `tick`, `accepted`, detail as typed OSC args).
They update state and emit outputs directly; they must not generate another input
that recording would then replay twice. Queued World actions run after ECS
systems and before the unchanged legacy move slice and Arena application update.
Legacy direct Rust World/app-action APIs remain available for existing callers;
the CLI, browser Shell and HTTP action forms use the queued command path.

Application actions are defined in `apps/demo-game/kitu-app-actions.toml`.
Unsupported legacy sample addresses return a diagnostic through the new live
path. `arena.start/menu/pause/resume/chest/close/inventory`, item transfer/equipment/upgrade
actions and `arena.use` affect the real game. Their ordered i32 parameters are
listed by `app action describe <id>`. Live
inputs and parameter application are refused during replay. A continuous Arena
frame is acknowledged as an input; its effect still follows the application's
pause/overlay rules, which are included in the returned state.

The initial `preparation-smoke` scenario is declared in
`apps/demo-game/content/arena-scenarios.json`: menu, start, paused management ticks
and explicit resume. Scenario waits observe the host clock and never tick it.
Actions and actual timing enter TSQ1 normally. A refusal stops the scenario and
reports applied steps; earlier effects are not rolled back. Concurrent operator
or Unity input may interleave, and its actual order is retained in recording.
Use TSQ1 playback for exact pre-recorded scenarios such as the 11F stock run.

## Identity and completion

`GET /shell/catalog` returns command version 1, a unique host-lifetime `sessionId`
and shared definitions. `POST /shell/execute` accepts:

```json
{"version":1,"sessionId":"<catalog session>","clientId":"operator-a","id":1,"args":["app","action","run","arena.start"]}
```

`POST /shell/line` uses the same fields with `line` instead of `args`. Stale
sessions, invalid versions/producer names, reused IDs with different arguments,
and lower first-seen IDs are rejected. Each terminal/browser producer advances a
positive ID. Identical retransmissions await or reuse the original result, even
when the original HTTP request disconnected. Commands continue in a host-owned
task; the request lifetime cannot cause a second execution.

Responses contain `{id, ok, data, error}`. An Arena rejection retains its complete
receipt and current state; its applied tick and sequence come from the game, not
from the request timestamp. State includes its own tick and may be newer than a
receipt if another tick has run. Continuous inputs and generic World actions also
receive a tick acknowledgment. Live receipts are retained independently of replay
presentation, including commands committed in the replay activation tick.

If a CLI call has an uncertain network result, retry with the printed `--session`,
`--client` and `--id` plus identical command arguments. Supplying `--session`
prevents a retry from acting on a newly restarted host. Do not invent a new ID to
retry a consumption/action. HTTP action forms share admission/result waiting but
are separate deliberate submissions; automatic retry/idempotency uses Shell.

## Initial bounds and verification

The development host retains at most 512 command identities for its lifetime;
when full, it refuses new commands instead of forgetting old IDs. Restart the
host for a fresh command session. Live receipt history retains 1024 entries,
input completion waits three seconds, and the CLI's overall request timeout is
90 seconds. Scenarios allow 32 steps and 600 waiting ticks. Existing recorder and
seek limits apply independently. No production remote service is implied.

[Stage 9 evidence](../verification/arena-shell/results.json) covers shared
quoting/type identity, duplicate execution, refusals, live scenarios and exact
TSQ1 state/events, real terminal and browser commands, and macOS Unity driven by
the actual CLI process. The Unity test uses `KITU_ARENA_CLI_EXECUTABLE` and optional
`KITU_ARENA_CLI_ARGUMENTS` in addition to the live/replay environment described in
the app README. Example for the isolated Docker host used in that evidence:

```sh
KITU_ARENA_CLI_EXECUTABLE=/usr/local/bin/docker
KITU_ARENA_CLI_ARGUMENTS='exec -w /workspaces/kitu-logic-processor kitu-e19c-arena-host target/debug/kitu-cli --endpoint http://127.0.0.1:8787'
```

Runtime source/lockfile changes create a new execution version. Re-generate the
stock TSQ1 using the documented replay test for the new build. Keep an old binary
or rebuild its pinned revision to verify older incompatible recordings; never
rewrite their saved execution identity. The stage 8 running binary was archived
locally before stage 9 activation for that purpose.
