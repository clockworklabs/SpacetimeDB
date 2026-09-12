# Environment the agent works in: Rust + SpacetimeDB CLI + Node 22, with a local
# SpacetimeDB instance started in the background by the entrypoint (so it's up for
# the agent and stays up for the SHARED-mode verifier).
# Rust 1.90: the spacetimedb crates require the `edition2024` Cargo feature
# (Rust >= 1.85, some deps want >= 1.87), so 1.83 is too old.
FROM rust:1.90-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Node 22 — supplies the global WebSocket the SpacetimeDB TS SDK uses.
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# wasm target for building SpacetimeDB modules.
RUN rustup target add wasm32-unknown-unknown

# SpacetimeDB CLI. TODO(spike): pin to 2.2.0 to match the committed bindings/SDK
# if the latest installer drifts and the WS protocol changes.
RUN curl -sSf https://install.spacetimedb.com | sh -s -- --yes
ENV PATH="/root/.local/bin:/root/.local/share/spacetime/bin/current:${PATH}"

RUN mkdir -p /var/lib/stdb
WORKDIR /app
EXPOSE 3000

# Background a standalone SpacetimeDB instance, then exec Harbor's keepalive command
# (sleep infinity, passed as args) so the agent and the SHARED-mode verifier share
# this one container with the server running.
ENTRYPOINT ["/bin/sh", "-c", "spacetime start --data-dir /var/lib/stdb --listen-addr 0.0.0.0:3000 --non-interactive >/var/log/spacetime.log 2>&1 & exec \"$@\"", "sh"]
CMD ["sh", "-c", "sleep infinity"]
