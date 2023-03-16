ARG CARGO_PROFILE=release

FROM rust:1.67 AS chef
RUN rust_target=$(rustc -vV | awk '/^host:/{ print $2 }') && \
  curl https://github.com/cargo-bins/cargo-binstall/releases/latest/download/cargo-binstall-$rust_target.tgz -fL | tar xz -C $CARGO_HOME/bin
RUN cargo binstall --no-confirm --no-symlinks cargo-chef@0.1.46
WORKDIR /usr/src/app

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS builder

RUN apt-get update && \
  apt-get install -y lsb-release wget software-properties-common
RUN wget https://apt.llvm.org/llvm.sh && \
  chmod +x llvm.sh && \
  ./llvm.sh 12

RUN apt-get update && apt-get install -y \
  cmake \
  less \
  linux-perf

RUN cargo binstall -y cargo-watch
RUN cargo binstall -y flamegraph

COPY --from=planner /usr/src/app/recipe.json .

ENV CARGO_INCREMENTAL=0

ARG CARGO_PROFILE=release

RUN cargo chef cook -p spacetimedb-standalone --profile=${CARGO_PROFILE}

COPY . .
RUN cargo build -p spacetimedb-standalone --profile=${CARGO_PROFILE} --locked

FROM builder as env-dev
ENV SPACETIMEDB_LOG_CONFIG=/usr/src/app/crates/standalone/log.conf
ENV PATH="/usr/src/app/target/debug:${PATH}"

FROM debian as env-release
RUN apt-get update && apt-get install -y libpython3.9 linux-perf
COPY --from=builder /usr/src/app/target/release/spacetimedb /usr/local/bin/
COPY --from=builder /usr/src/app/crates/standalone/log.conf /etc/spacetimedb/

FROM env-${CARGO_PROFILE}

EXPOSE 3000

ENV RUST_BACKTRACE=1
ENTRYPOINT ["spacetimedb"]
