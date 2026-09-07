#!/usr/bin/env bash
set -euo pipefail
git lfs install --local
git lfs version
git lfs pull

# Node 24 is supplied by the Dev Container feature. Corepack uses the same pnpm
# version as the frontend packageManager field and its frozen lockfile.
rustup show
rustup target add wasm32-unknown-unknown --toolchain 1.96.0
corepack enable
corepack prepare pnpm@11.9.0 --activate
rustc --version
cargo --version
node --version
pnpm --version
python3 --version
