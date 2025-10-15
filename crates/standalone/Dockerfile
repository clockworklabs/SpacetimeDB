ARG CARGO_PROFILE=release


FROM rust:1.90.0 AS chef
RUN rust_target=$(rustc -vV | awk '/^host:/{ print $2 }') && \
  curl https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-$rust_target.tgz -fL | tar xz -C $CARGO_HOME/bin
RUN cargo binstall -y cargo-chef@0.1.70
WORKDIR /usr/src/app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

RUN cargo binstall -y cargo-watch@8.4.0
RUN cargo binstall -y flamegraph@0.6.2

COPY --from=planner /usr/src/app/recipe.json .

ENV CARGO_INCREMENTAL=0

ARG CARGO_PROFILE=release

RUN cargo chef cook -p spacetimedb-standalone --profile=${CARGO_PROFILE}

COPY . .
RUN cargo build -p spacetimedb-standalone --profile=${CARGO_PROFILE} --locked

FROM builder as env-dev
RUN mkdir -p /stdb/data && ln -s /usr/src/app/crates/standalone/config.toml /stdb/data/config.toml
ENV PATH="/usr/src/app/target/debug:${PATH}"

FROM debian as env-release
RUN apt-get update && \
    apt-get install -y ca-certificates libssl-dev && \
    rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/src/app/target/release/spacetimedb-standalone /usr/local/bin/
COPY --from=builder /usr/src/app/crates/standalone/config.toml /stdb/data/config.toml

FROM env-${CARGO_PROFILE}

EXPOSE 3000

ENV RUST_BACKTRACE=1
ENTRYPOINT ["spacetimedb-standalone"]
CMD ["start", "--data-dir=/stdb/data", "--jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub", "--jwt-priv-key-path=/etc/spacetimedb/id_ecdsa"]
