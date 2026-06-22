#!/usr/bin/env sh
set -eu

repo_root="$(CDPATH= cd -- "$(dirname -- "$0")/../../.." && pwd)"

docker run --rm \
  -v "$repo_root:/workspace" \
  -w /workspace/tools/kitu-webtransport-gateway \
  rust:1.88-bookworm \
  cargo check --locked --bins
