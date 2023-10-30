# dockerfile for deterministic benchmarking environment

FROM rust:1.72

RUN apt-get update && \
    apt-get install -y valgrind protobuf-compiler bash && \
    rm -rf /var/lib/apt/lists/*
#RUN cargo install --git git@github.com:clockworklabs/iai-callgrind.git --branch main iai-callgrind-runner

ENV CARGO_TARGET_DIR=/projects/SpacetimeDB/linux-target
ENV CARGO_HOME=/projects/SpacetimeDB/linux-cache
ENV RUSTUP_HOME=/projects/SpacetimeDB/linux-rustup
