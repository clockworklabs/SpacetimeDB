FROM rust:1.90.0
RUN mkdir -p /stdb/data
COPY ./target/debug/spacetimedb-standalone ./target/debug/spacetimedb-cli /usr/local/bin/
COPY ./crates/standalone/config.toml /stdb/data/config.toml
