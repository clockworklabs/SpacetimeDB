FROM rust:bookworm

RUN curl -sSfLO https://packages.microsoft.com/config/debian/12/packages-microsoft-prod.deb && \
      dpkg -i packages-microsoft-prod.deb && \
      rm packages-microsoft-prod.deb && \
      apt-get update && \
      apt-get install -y dotnet-sdk-9.0 binaryen && \
      dotnet workload install wasi-experimental

RUN rustup target add wasm32-unknown-unknown

RUN useradd -m spacetime
USER spacetime
RUN curl -sSfL https://install.spacetimedb.com | bash -s -- --yes
ENV PATH="/home/spacetime/.local/bin:${PATH}"
WORKDIR /app

EXPOSE 3000

ENTRYPOINT ["spacetime"]
