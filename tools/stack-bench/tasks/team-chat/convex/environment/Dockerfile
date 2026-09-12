# Environment the agent works in: the official self-hosted Convex backend image
# (Ubuntu 24.04, /convex/convex-local-backend + run scripts) with Node 22 added
# for the agent's `npx convex` tooling and the verifier. Single container so
# the verifier can kill and restart the backend process for durability checks.
FROM ghcr.io/get-convex/convex-backend:latest

USER root
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates procps \
    && curl -fsSL https://deb.nodesource.com/setup_22.x | bash - \
    && apt-get install -y --no-install-recommends nodejs \
    && rm -rf /var/lib/apt/lists/*

COPY backendctl /opt/stack-bench/backendctl
RUN chmod +x /opt/stack-bench/backendctl && mkdir -p /var/log

WORKDIR /app
EXPOSE 3210 3211

# Boot the backend via backendctl, then exec Harbor's keepalive command so the
# agent and the SHARED-mode verifier share this container with the backend up.
ENTRYPOINT ["/bin/sh", "-c", "/opt/stack-bench/backendctl start && exec \"$@\"", "sh"]
CMD ["sh", "-c", "sleep infinity"]
