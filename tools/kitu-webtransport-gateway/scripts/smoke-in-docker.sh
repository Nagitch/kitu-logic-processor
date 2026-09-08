#!/usr/bin/env sh
set -eu

compose_file="${KITU_DEMO_COMPOSE_FILE:-}"
if [ "${1:-}" = "--compose-file" ]; then
  if [ "$#" -ne 2 ]; then
    printf '%s\n' 'Usage: smoke-in-docker.sh --compose-file /absolute/demo/docker-compose.yml' >&2
    exit 2
  fi
  compose_file=$2
  shift 2
fi
if [ "$#" -ne 0 ] || [ -z "$compose_file" ]; then
  printf '%s\n' 'A demo Compose file is required: pass --compose-file PATH or set KITU_DEMO_COMPOSE_FILE.' >&2
  exit 2
fi
if [ ! -f "$compose_file" ]; then
  printf 'Demo Compose file does not exist: %s\n' "$compose_file" >&2
  exit 2
fi

demo_root="$(CDPATH= cd -- "$(dirname -- "$compose_file")" && pwd)"
compose_file="$demo_root/$(basename -- "$compose_file")"
compose_helper="$demo_root/tools/compose.py"
if [ ! -f "$compose_helper" ]; then
  printf 'Standalone demo Compose helper does not exist: %s\n' "$compose_helper" >&2
  exit 2
fi

compose() {
  python3 "$compose_helper" --file "$compose_file" --profile webtransport "$@"
}

run_client() {
  client=$1
  shift
  compose run --rm "$@" gateway-smoke sh -ec '
    cert_hash="$(openssl x509 -in /certs/webtransport-cert.pem -outform der | openssl dgst -sha256 -r | awk "{print \$1}")"
    export PUBLIC_KITU_ADMIN_WT_CERT_SHA256="$cert_hash" KITU_WT_SMOKE_CERT_SHA256="$cert_hash"
    exec cargo run --locked --bin "$1"
  ' sh "$client"
}

compose up -d --build --force-recreate demo-game webtransport-gateway

for attempt in $(seq 1 60); do
  if curl -fsS http://localhost:8787/health >/dev/null 2>&1; then
    break
  fi
  if [ "$attempt" -eq 60 ]; then
    printf '%s\n' 'demo-game did not become healthy.' >&2
    exit 1
  fi
  sleep 1
done

run_client kitu-webtransport-gateway-smoke-client \
  -e KITU_WT_SMOKE_URL=https://webtransport-gateway:9443 \
  -e KITU_WT_SMOKE_OBJECT_ID=webtransport-smoke

curl -fsS http://localhost:8787/state | grep -q '"kind":"webtransport-smoke-0"'
curl -fsS http://localhost:8787/state | grep -q '"kind":"webtransport-smoke-1"'

printf '%s\n' 'WebTransport gateway smoke test passed.'
