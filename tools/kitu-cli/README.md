# kitu-cli

A terminal client and REPL for a running Kitu development host. Build with
`cargo build -p kitu-cli`; run `kitu-cli help` or `kitu-cli shell`.
`--endpoint` / `KITU_RUNTIME_URL` selects the host (default
`http://127.0.0.1:8787`). Commands and results are shared with the browser Shell.

See [live commands](../../doc/specs/live-shell.md) for examples, exact retry
identity, applied outcomes and bounds. The executable does not construct a local
simulation or merely print a materialized action. JSON goes to stdout and
refusals return a nonzero status. This first client is for local HTTP development.
