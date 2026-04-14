# Use a base image that supports multi-arch
FROM rust:bookworm AS builder

WORKDIR /usr/src/app
COPY . .

# If we're in a git submodule, we'll have a corrupted/nonfunctional .git file instead of a proper .git directory.
# To make the errors more sane, remove .git entirely.
RUN if [ -f .git ]; then \
      echo "❌ ERROR: .git is a file (likely a submodule pointer), not a directory." >&2; \
      echo "This will cause errors in the build process, because git operations will fail." >&2; \
      echo "To address this, replace the .git file with a proper .git directory." >&2; \
      exit 1; \
    fi

RUN cargo build -p spacetimedb-standalone -p spacetimedb-cli --release --locked

FROM rust:bookworm

# Install dependencies
RUN apt-get update && apt-get install -y \
      curl \
      ca-certificates \
      binaryen \
      build-essential \
      && rm -rf /var/lib/apt/lists/*

# Determine architecture for .NET installation
ARG TARGETARCH
ENV DOTNET_ARCH=${TARGETARCH}

RUN if [ "$DOTNET_ARCH" = "amd64" ]; then \
        DOTNET_ARCH="x64"; \
    elif [ "$DOTNET_ARCH" = "arm64" ]; then \
        DOTNET_ARCH="arm64"; \
    else \
        echo "Unsupported architecture: $DOTNET_ARCH" && exit 1; \
    fi && \
    curl -sSL https://dot.net/v1/dotnet-install.sh | bash /dev/stdin --channel 8.0 --install-dir /usr/share/dotnet --architecture $DOTNET_ARCH

ENV PATH="/usr/share/dotnet:${PATH}"

# Install the experimental WASI workload
RUN dotnet workload install wasi-experimental

# Install Rust WASM target
RUN rustup target add wasm32-unknown-unknown

# Copy over SpacetimeDB
COPY --from=builder --chmod=755 /usr/src/app/target/release/spacetimedb-standalone /usr/src/app/target/release/spacetimedb-cli /opt/spacetime/
RUN ln -s /opt/spacetime/spacetimedb-cli /usr/local/bin/spacetime

# Create and switch to a non-root user
RUN useradd -m spacetime
USER spacetime

# Set working directory
WORKDIR /app

# Expose the necessary port
EXPOSE 3000

# Define the entrypoint
ENTRYPOINT ["spacetime"]

