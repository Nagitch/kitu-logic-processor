# Live Kitu CLI and browser Shell

`kitu-cli` and the browser Admin Shell connect to an application host through
shared `kitu-shell` contracts. `kitu-shell` owns the catalog, argument
validation, quoting, typed OSC conversion, and request/result definitions. The
browser sends its line to the same Rust parser used by the CLI REPL. A host must
never start an OS shell or expand variables, substitutions, wildcards, or
scripts from command text.

Application hosts are independent consumers. The maintained implementation and
end-to-end evidence live in
[Nagitch/kitu-unity-demo-game](https://github.com/Nagitch/kitu-unity-demo-game).

## Build and operate the client

Build the framework CLI inside the Kitu development environment:

```sh
cargo build -p kitu-cli --bin kitu-cli
```

Start a compatible application host separately, then use its HTTP endpoint:

```sh
target/debug/kitu-cli --endpoint http://127.0.0.1:8787 help
target/debug/kitu-cli --endpoint http://127.0.0.1:8787 inspect application
target/debug/kitu-cli --endpoint http://127.0.0.1:8787 app action list
target/debug/kitu-cli --endpoint http://127.0.0.1:8787 shell
```

`KITU_RUNTIME_URL` supplies the endpoint default. `--help` is available offline;
`help` fetches the connected host's definitions. JSON results go to stdout and
connection/request identities to stderr. Refusal or failure exits nonzero. The
REPL accepts one command per line, `exit`, and quoted strings; piped scripts keep
reading and return nonzero if any command failed.

The browser Shell shows the same definitions, recent results and refusals,
complete JSON, and Up/Down history. Reconnect refreshes the host identity. Failed
HTTP calls retain the exact request for **Retry same request**; they are never
silently resent under a new ID.

## Commands and ownership

Use `help` for the authoritative catalog. The shared command families are:

| Command | Observable result |
| --- | --- |
| `inspect application/world/content/recording/replay` | State, recorded tick, settings, or replay diagnostics from the active host. |
| `osc send <address> [typed arguments]` | Normal input admission and applied outcome. |
| `app action list/describe/run` | Runtime catalog and application-owned actions. |
| `content validate/stage` | Application content validation and reviewed next-run staging. |
| `inspect script`, `script validate/stage` | Active/pending source versions and bounded validation. |
| `inspect timeline`, `timeline validate/stage` | Clip versions, cue positions, and detached staging. |
| `replay list/save/verify/load/play/pause/step/stop/seek/live` | Recording and playback operations. |
| `scenario list/run` | Application-owned bounded action sequences. |

OSC types are explicit: `i:` i32, `h:` i64, `f:` finite f32, `s:` string, and
`b:` `true`/`false`.

Generic World actions are applied at the Runtime tick, with ordered outcome
receipts at `/ui/kitu/command` (`sequence`, `tick`, `accepted`, and detail as
typed OSC args). They update state and emit outputs directly; they must not
generate another input that recording would replay twice. The Admin App Actions
page posts typed inputs to `/app-actions/{id}/run`; server-side catalog
materialization and applied receipts determine the displayed result.

Applications own action definitions, scenario data, pause/overlay rules, and
admission policy. Live inputs and parameter application may be refused during
replay. A refusal stops a scenario and reports applied steps; earlier effects are
not rolled back. Concurrent operator or engine input may interleave, and the
recording retains its actual order.

## Identity and completion

`GET /shell/catalog` returns command version 1, a unique host-lifetime
`sessionId`, and shared definitions. `POST /shell/execute` accepts identified
argument arrays; `POST /shell/line` uses the same identity fields with `line`
instead of `args`:

```json
{
  "version": 1,
  "sessionId": "<catalog session>",
  "clientId": "operator-a",
  "id": 1,
  "line": "inspect application"
}
```

Stale sessions, invalid versions or producer names, reused IDs with different
arguments, and lower first-seen IDs are rejected. Each terminal or browser
producer advances a positive ID. Identical retransmissions await or reuse the
original result, even when the original HTTP request disconnected. Commands
continue in a host-owned task; request lifetime cannot cause a second execution.

Responses contain `{id, ok, data, error}`. A rejection retains its complete
receipt and current state. Its applied tick and sequence come from the
application, not from the request timestamp. If a CLI call has an uncertain
network result, retry with the printed `--session`, `--client`, and `--id` plus
identical command arguments. Supplying `--session` prevents a retry from acting
on a newly restarted host.

## Bounds

The reference development host retains at most 512 command identities for its
lifetime and refuses new commands when full. Live receipt history retains 1024
entries, input completion waits three seconds, and the CLI request timeout is 90
seconds. Applications may impose tighter scenario, recorder, and seek limits.
No production authentication or remote-service policy is implied by this local
contract.
