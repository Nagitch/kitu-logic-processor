# Kitu Integration Runner

This directory retains the framework-level integration and replay contract.
The generic smoke fixture remains at `scenarios/smoke/player-move-basic/` and
`tools/kitu-replay-runner` executes it without an application checkout.

Use [`doc/specs/integration-replay-framework.md`](../doc/specs/integration-replay-framework.md)
as the source of truth for scenario, expected-output and report formats.
Generated artifacts belong outside checked-in fixtures, for example under a
caller-provided `--output-dir`.

The Endless Arena application and Unity verification project were extracted to
[Nagitch/kitu-unity-demo-game](https://github.com/Nagitch/kitu-unity-demo-game).
This runner must remain generic and must not acquire Arena rules, application
fixtures or a default application Compose file.
