FROM mcr.microsoft.com/devcontainers/rust:1-bullseye

# Core build and troubleshooting tools
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        clang \
        lld \
        pkg-config \
        libssl-dev \
        git \
        zsh \
        fzf \
        ripgrep \
    && rm -rf /var/lib/apt/lists/*

# Ensure required Rust components and helper tools are available
RUN rustup component add clippy rustfmt \
    && if ! command -v just >/dev/null 2>&1; then cargo install --locked just; fi
