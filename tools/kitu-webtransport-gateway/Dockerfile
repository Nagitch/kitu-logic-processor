FROM rust:1.96.0-bookworm

WORKDIR /workspace/tools/kitu-webtransport-gateway

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse

CMD ["cargo", "run", "--locked", "--bin", "kitu-webtransport-gateway"]
