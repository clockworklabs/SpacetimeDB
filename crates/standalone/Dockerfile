FROM rust:1.90.0
RUN mkdir -p /stdb/data
COPY ./target/debug/spacetimedb-standalone ./target/debug/spacetimedb-cli /usr/local/bin/
COPY ./crates/standalone/config.toml /stdb/data/config.toml

EXPOSE 3000

ENV RUST_BACKTRACE=1
ENTRYPOINT ["spacetimedb-standalone"]
CMD ["start", "--data-dir=/stdb/data", "--jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub", "--jwt-priv-key-path=/etc/spacetimedb/id_ecdsa"]
