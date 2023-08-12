FROM rust:1.69.0

WORKDIR /usr/src/app
RUN apt-get update && apt-get install -y protobuf-compiler

RUN cargo install cargo-watch@8.4.0
RUN cargo install flamegraph@0.6.2

COPY . .
COPY crates/standalone/log.conf /etc/spacetimedb/

ENV CARGO_INCREMENTAL=0
ARG CARGO_PROFILE=release
ENV SPACETIMEDB_LOG_CONFIG=/usr/src/app/crates/standalone/log.conf
ENV SPACETIMEDB_JWT_PUB_KEY=/etc/spacetimedb/id_ecdsa.pub
ENV SPACETIMEDB_JWT_PRIV_KEY=/etc/spacetimedb/id_ecdsa

# We need the container to be able to find the spacetimedb binary regardless
# of whether we are building in debug or release
ENV PATH="/usr/src/app/target/debug:${PATH}"
ENV PATH="/usr/src/app/target/release:${PATH}"

RUN cargo build -p spacetimedb-standalone --profile=${CARGO_PROFILE} --locked

EXPOSE 3000

ENV RUST_BACKTRACE=1
ENTRYPOINT ["spacetimedb"]
CMD ["start"]
