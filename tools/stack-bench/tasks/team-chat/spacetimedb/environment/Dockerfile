# Environment the agent works in: Rust + SpacetimeDB CLI + Node 22, with a local
# SpacetimeDB instance managed by /opt/stack-bench/backendctl (started at boot,
# restarted by the verifier's durability phase).
# Rust 1.93: the spacetimedb 2.7.x crates require rustc >= 1.93.
FROM rust:1.93-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates procps \
    && rm -rf /var/lib/apt/lists/*

# Node 22 — supplies the global WebSocket the SpacetimeDB TS SDK uses.
RUN curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

# wasm target for building SpacetimeDB modules.
RUN rustup target add wasm32-unknown-unknown

# SpacetimeDB CLI. The committed module_bindings + pinned npm SDK are 2.5.x;
# if the installer's latest drifts incompatibly, pin here and regenerate.
RUN curl -sSf https://install.spacetimedb.com | sh -s -- --yes
ENV PATH="/root/.local/bin:/root/.local/share/spacetime/bin/current:${PATH}"

COPY backendctl /opt/stack-bench/backendctl
RUN chmod +x /opt/stack-bench/backendctl && mkdir -p /var/lib/stdb
WORKDIR /app
EXPOSE 3000

# Boot the backend via backendctl, then exec Harbor's keepalive command so the
# agent and the SHARED-mode verifier share this container with the server up.
ENTRYPOINT ["/bin/sh", "-c", "/opt/stack-bench/backendctl start && exec \"$@\"", "sh"]
CMD ["sh", "-c", "sleep infinity"]
