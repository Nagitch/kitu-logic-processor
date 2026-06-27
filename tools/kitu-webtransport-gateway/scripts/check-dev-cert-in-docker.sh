#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"

docker run --rm \
  -v "$repo_root:/workspace" \
  -w /workspace/tools/kitu-webtransport-gateway \
  rust:1.88-bookworm \
  sh -eu -c '
    cert="certs/webtransport-cert.pem"
    key="certs/webtransport-key.pem"
    env_file="certs/webtransport.env"
    hash_file="certs/webtransport-cert.sha256"

    for path in "$cert" "$key" "$env_file" "$hash_file"; do
      if [ ! -f "$path" ]; then
        printf "%s\n" "Missing $path. Run tools/kitu-webtransport-gateway/scripts/generate-dev-cert-in-docker.sh first." >&2
        exit 1
      fi
    done

    cert_hash="$(openssl x509 -in "$cert" -outform der \
      | openssl dgst -sha256 -r \
      | awk "{print \$1}")"
    file_hash="$(tr -d "[:space:]" < "$hash_file")"
    public_env_hash="$(awk -F= "/^PUBLIC_KITU_ADMIN_WT_CERT_SHA256=/ {print \$2}" "$env_file")"
    smoke_env_hash="$(awk -F= "/^KITU_WT_SMOKE_CERT_SHA256=/ {print \$2}" "$env_file")"

    for value in "$file_hash" "$public_env_hash" "$smoke_env_hash"; do
      if ! printf "%s" "$value" | grep -Eq "^[0-9a-fA-F]{64}$"; then
        printf "%s\n" "Invalid WebTransport certificate hash in generated files. Regenerate the dev certificate." >&2
        exit 1
      fi
    done

    if [ "$cert_hash" != "$file_hash" ] || \
       [ "$cert_hash" != "$public_env_hash" ] || \
       [ "$cert_hash" != "$smoke_env_hash" ]; then
      printf "%s\n" "Generated WebTransport certificate hash files do not match. Regenerate the dev certificate." >&2
      exit 1
    fi

    cert_pubkey_hash="$(openssl x509 -in "$cert" -pubkey -noout \
      | openssl pkey -pubin -outform der \
      | openssl dgst -sha256 -r \
      | awk "{print \$1}")"
    key_pubkey_hash="$(openssl pkey -in "$key" -pubout -outform der \
      | openssl dgst -sha256 -r \
      | awk "{print \$1}")"
    if [ "$cert_pubkey_hash" != "$key_pubkey_hash" ]; then
      printf "%s\n" "Generated WebTransport certificate and key do not match. Regenerate the dev certificate." >&2
      exit 1
    fi

    if ! openssl x509 -in "$cert" -checkend 86400 -noout >/dev/null; then
      printf "%s\n" "Generated WebTransport certificate expires within 24 hours. Regenerate it before browser testing." >&2
      exit 1
    fi

    printf "%s\n" "WebTransport dev certificate files are consistent."
    printf "Certificate SHA-256: %s\n" "$cert_hash"
    printf "Certificate expires: "
    openssl x509 -in "$cert" -noout -enddate | sed "s/^notAfter=//"
  '
