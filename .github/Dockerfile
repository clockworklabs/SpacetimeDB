# Minimal Dockerfile that just wraps pre-built binaries, so we can test the server inside docker
FROM rust:1.93.0
# Refresh OS roots and rustup's bundled roots to distrust compromised eMudhra CAs.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
RUN rustup --version && rustup self update && rustup --version
RUN mkdir -p /stdb/data
COPY ./target/debug/spacetimedb-standalone ./target/debug/spacetimedb-cli /usr/local/bin/
COPY ./crates/standalone/config.toml /stdb/data/config.toml
RUN ln -s /usr/local/bin/spacetimedb-cli /usr/local/bin/spacetime
