# TODO maybe a multistage build eventually?
FROM rust:1.57.0
WORKDIR /usr/src/app

RUN cargo install cargo-watch
RUN apt-get update && apt-get install -y \
  cmake \
  less

COPY ./src ./src
COPY ./code-gen ./code-gen
COPY ./bitcraft/benches ./bitcraft/benches
COPY ./cheats ./cheats

COPY ./bitcraft/Cargo.toml ./bitcraft/
COPY ./Cargo.lock ./
COPY ./Cargo.toml ./

RUN cd bitcraft && echo "fn main() {}" > dummy.rs
RUN cd bitcraft && sed -i 's#src/main.rs#dummy.rs#' Cargo.toml
RUN cd bitcraft && cargo install --locked --path .
RUN cd bitcraft && sed -i 's#dummy.rs#src/main.rs#' Cargo.toml

COPY ./bitcraft ./bitcraft

# NOTE: This will still have to compile some additional dependencies
# because of the build.rs file in the bitcraft project
RUN cd bitcraft && cargo install --locked --path .
RUN cd cheats && cargo install --locked --path .

EXPOSE 3000

WORKDIR /usr/src/app/bitcraft
ENV RUST_BACKTRACE=1
CMD ["../target/release/bitcraft"]
