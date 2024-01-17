# Dockerfile for callgrind benchmarking environment.
# Set up to run from linux / WSL (running from a windows file system will be extremely slow).
# See the README for commands to run.

# sync with: ../../rust-toolchain.toml
FROM rust:1.74

RUN apt-get update && \
    apt-get install -y valgrind protobuf-compiler bash && \
    rm -rf /var/lib/apt/lists/*

ENV CARGO_TARGET_DIR=/projects/SpacetimeDB/linux-target
ENV CARGO_HOME=/projects/SpacetimeDB/linux-cache
ENV RUSTUP_HOME=/projects/SpacetimeDB/linux-rustup

RUN rustup default 1.74 && cargo install --git https://github.com/clockworklabs/iai-callgrind.git --branch main iai-callgrind-runner
