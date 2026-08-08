# The agent's container ("main"). Node 22 (global WebSocket for the Convex client);
# `npx convex` is used at deploy time. The Convex backend itself runs as a separate
# compose service (see docker-compose.yaml).
FROM node:22-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app
