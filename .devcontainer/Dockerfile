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

# Fix cargo registry permissions to avoid "Permission denied" errors
# Ensure vscode user can write to cargo registry directories
# Note: The base image provides the 'rustlang' group and adds 'vscode' to it
RUN mkdir -p /usr/local/cargo/registry/cache \
    && mkdir -p /usr/local/cargo/registry/index \
    && mkdir -p /usr/local/cargo/registry/src \
    && chown -R vscode:rustlang /usr/local/cargo/registry \
    && chmod -R ug+rw /usr/local/cargo/registry
