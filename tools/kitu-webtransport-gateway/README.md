# WebTransport gateway scripts

The gateway is a framework transport experiment. Its Docker smoke and
integration scripts accept the standalone demo Compose file explicitly:

```sh
tools/kitu-webtransport-gateway/scripts/smoke-in-docker.sh \
  --compose-file /absolute/path/to/kitu-unity-demo-game/docker-compose.yml

KITU_DEMO_COMPOSE_FILE=/absolute/path/to/kitu-unity-demo-game/docker-compose.yml \
  tools/kitu-webtransport-gateway/scripts/integration-in-docker.sh
```

The selected Compose file must be beside the demo's `tools/compose.py`. The
scripts invoke that helper to derive `KITU_REPOSITORY` and `KITU_REV` from the
demo's Cargo manifest. App, gateway and clients therefore use the demo's fixed
Kitu revision, which may differ from the checkout containing these wrappers.
For local Kitu overrides, use the demo's host/native verification workflow.

The gateway defaults `KITU_GATEWAY_INTERNAL_WS_URL` to
`ws://127.0.0.1:8787/ws` for a host running in the same environment. Compose
stacks must set the application service URL explicitly.

The demo owns the `demo-game`, `webtransport-gateway` and `gateway-smoke`
services and their `gateway-certs` volume. Client commands run in `gateway-smoke`
and read the certificate hash from the same mounted certificate used by the
server. No certificate under the caller's framework checkout is substituted.
The integration script retains its readiness check and valid/invalid KEP tests.
