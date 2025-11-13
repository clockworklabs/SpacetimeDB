# Use a base image that supports multi-arch
FROM rust:slim AS builder

WORKDIR /usr/src/app
COPY . .

FROM rust:slim

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

