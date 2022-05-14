FROM --platform=$BUILDPLATFORM rust:1.58.0 AS rust
WORKDIR /usr/src/app

RUN cargo install cargo-watch

RUN apt update
RUN apt install -y lsb-release wget software-properties-common
RUN wget https://apt.llvm.org/llvm.sh
RUN chmod +x llvm.sh
RUN ./llvm.sh 12

RUN apt-get update && apt-get install -y \
  cmake \
  less

COPY ./crates/spacetimedb-bindings ../spacetimedb-bindings
COPY ./crates/spacetimedb/src ./src
COPY ./crates/spacetimedb/protobuf ./protobuf

COPY ./Cargo.lock ./
COPY ./crates/spacetimedb/Cargo.toml ./

RUN echo "fn main() {}" > dummy.rs
RUN sed -i 's#src/main.rs#dummy.rs#' Cargo.toml
RUN cargo install --locked --path .
RUN sed -i 's#dummy.rs#src/main.rs#' Cargo.toml

COPY ./crates/spacetimedb ./

RUN cargo install --locked --path .

EXPOSE 3000

WORKDIR /usr/src/app
ENV RUST_BACKTRACE=1
CMD ["../target/release/spacetimedb"]
