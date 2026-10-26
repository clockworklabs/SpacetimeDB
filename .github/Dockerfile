# Minimal Dockerfile that just wraps pre-built binaries, so we can test the server inside docker
FROM rust:1.93.0
# Refresh OS certificate roots to distrust the compromised eMudhra CAs.
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/*
# rustup has its own CA bundle; 1.29.2+ distrusts the compromised eMudhra CAs.
RUN rustup --version && rustup self update && rustup --version
RUN mkdir -p /stdb/data
COPY ./target/debug/spacetimedb-standalone ./target/debug/spacetimedb-cli /usr/local/bin/
COPY ./crates/standalone/config.toml /stdb/data/config.toml
RUN ln -s /usr/local/bin/spacetimedb-cli /usr/local/bin/spacetime
