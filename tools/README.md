# Tools

Binary utilities for interacting with the Kitu runtime will live in this directory. Current entries are skeletons for
`kitu-cli` and `kitu-replay-runner`.

## Replay runner

`kitu-replay-runner` runs a checked-in scenario through the runtime boundary, compares the observed outputs with an
expected-output fixture, and writes `summary.json`.

```sh
cargo run -p kitu-replay-runner -- \
  --scenario kitu-integration-runner/scenarios/smoke/player-move-basic/scenario.json \
  --expected kitu-integration-runner/scenarios/smoke/player-move-basic/expected.json \
  --output-dir /tmp/kitu-replay-player-move-basic
```
