# TODO maybe a multistage build eventually?
FROM rust:1.57.0
WORKDIR /usr/src/app

RUN cargo install cargo-watch

RUN apt update
RUN apt install -y lsb-release wget software-properties-common
RUN wget https://apt.llvm.org/llvm.sh
RUN chmod +x llvm.sh
RUN ./llvm.sh 12

# RUN apt update
# RUN apt install -y software-properties-common
# RUN wget --no-check-certificate -O - https://apt.llvm.org/llvm-snapshot.gpg.key | apt-key add -
# RUN apt-add-repository "deb http://apt.llvm.org/bionic/ llvm-toolchain-bionic-12 main"
# COPY ./crates/spacetimedb/libffi6_3.2.1-9_arm64.deb ./
# RUN apt-get install -y ./libffi6_3.2.1-9_arm64.deb
# RUN apt-get update && apt-get install -y libllvm12 llvm-12 llvm-12-dev llvm-12-doc llvm-12-examples llvm-12-runtime
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
