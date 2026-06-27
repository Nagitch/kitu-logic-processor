#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
compose_file="$repo_root/apps/demo-game/docker-compose.yml"

if [ ! -f "$repo_root/tools/kitu-webtransport-gateway/certs/webtransport.env" ]; then
  "$repo_root/tools/kitu-webtransport-gateway/scripts/generate-dev-cert-in-docker.sh"
fi

docker compose -f "$compose_file" up -d --build --force-recreate demo-game webtransport-gateway

for attempt in $(seq 1 60); do
  if curl -fsS http://localhost:8787/health >/dev/null 2>&1; then
    break
  fi
  if [ "$attempt" -eq 60 ]; then
    printf "%s\n" "demo-game did not become healthy." >&2
    exit 1
  fi
  sleep 1
done

docker compose -f "$compose_file" run --rm \
  -e KITU_WT_INTEGRATION_URL=https://webtransport-gateway:9443 \
  -e KITU_WT_INTEGRATION_OBJECT_ID=webtransport-integration \
  webtransport-gateway \
  cargo run --locked --bin kitu-webtransport-gateway-integration-client

curl -fsS http://localhost:8787/state \
  | grep -q '"kind":"webtransport-integration"'

printf "%s\n" "WebTransport gateway Docker integration test passed."
