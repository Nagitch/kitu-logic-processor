#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"
cert_dir="$repo_root/tools/kitu-webtransport-gateway/certs"

mkdir -p "$cert_dir"

docker run --rm \
  -v "$repo_root:/workspace" \
  -w /workspace/tools/kitu-webtransport-gateway \
  rust:1.88-bookworm \
  sh -eu -c '
    mkdir -p certs
    openssl ecparam -name prime256v1 -genkey -noout -out certs/webtransport-key.pem
    openssl req \
      -new \
      -x509 \
      -key certs/webtransport-key.pem \
      -sha256 \
      -days 13 \
      -out certs/webtransport-cert.pem \
      -subj "/CN=localhost" \
      -addext "subjectAltName=DNS:localhost,IP:127.0.0.1"

    cert_hash="$(openssl x509 -in certs/webtransport-cert.pem -outform der \
      | openssl dgst -sha256 -r \
      | awk "{print \$1}")"

    {
      printf "%s\n" "KITU_WT_GATEWAY_CERT=/workspace/tools/kitu-webtransport-gateway/certs/webtransport-cert.pem"
      printf "%s\n" "KITU_WT_GATEWAY_KEY=/workspace/tools/kitu-webtransport-gateway/certs/webtransport-key.pem"
      printf "%s\n" "PUBLIC_KITU_ADMIN_WT_CERT_SHA256=$cert_hash"
      printf "%s\n" "KITU_WT_SMOKE_CERT_SHA256=$cert_hash"
    } > certs/webtransport.env

    printf "%s\n" "$cert_hash" > certs/webtransport-cert.sha256
    chmod 600 certs/webtransport-key.pem
  '

printf "Generated WebTransport dev certificate files in %s\n" "$cert_dir"
printf "Certificate SHA-256: "
cat "$cert_dir/webtransport-cert.sha256"
printf "%s\n" "Run tools/kitu-webtransport-gateway/scripts/check-dev-cert-in-docker.sh to verify the generated files."
