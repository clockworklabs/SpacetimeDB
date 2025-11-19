# Minimal Dockerfile that just wraps pre-built binaries, so we can test the server inside docker
FROM rust:1.90.0
RUN mkdir -p /stdb/data
COPY ./target/debug/spacetimedb-standalone ./target/debug/spacetimedb-cli /usr/local/bin/
COPY ./crates/standalone/config.toml /stdb/data/config.toml
RUN ln -s /usr/local/bin/spacetimedb-cli /usr/local/bin/spacetime
