# The image a generated app is built in.
#
# It exists so the harness that grades a build is not on the filesystem the
# build can see. A fix round once read the scenario file defining the criteria
# it was failing, read grade.mjs, and ran the grader — 23 accesses, all through
# Bash, which the permission system does not govern. Denying paths is a
# blocklist; absence is not.
#
# Nothing from tools/stack-bench is copied in or mounted at run time. The only
# repository paths a container sees are the two artifacts actually under test,
# mounted read-only by run-build.mjs.
FROM node:22-slim

# git: builds initialise repositories and some tooling shells out to it.
# curl: readiness probes against the app's own dev server.
# ca-certificates: TLS for npm and the API.
# procps: the build starts and stops its own dev servers.
# lsof: `kill-port` locates a listener with lsof on Linux, and finds nothing
# without it — it then prints "Process on port N killed" and exits 0 while the
# server keeps running. Every durability and deploy-window test would pass
# without restarting anything, which is worse than failing. None of lsof, fuser,
# ss or netstat is present in node:22-slim.
RUN apt-get update && apt-get install -y --no-install-recommends \
      git curl ca-certificates procps lsof \
    && rm -rf /var/lib/apt/lists/*

# Pinned, and the auto-updater disabled: a CLI that updates itself mid-series
# changes the thing under test between one backend and the next. Override at
# build time to move deliberately rather than by drift.
ARG CLAUDE_VERSION=2.1.226
ENV DISABLE_AUTOUPDATER=1
RUN npm install -g @anthropic-ai/claude-code@${CLAUDE_VERSION} \
    && claude --version

# Cache reads are 97-98% of a run's bill, so the cache tier moves cost more than
# anything else measured. Pinned here for the same reason it is pinned on the
# host: on the 1-hour tier a second run of the same backend within the hour
# reads a prefix the first paid to create and looks cheaper for reasons that
# have nothing to do with the database.
ENV FORCE_PROMPT_CACHING_5M=1

# The app under construction. Everything the build writes lives here, and the
# host mounts its own work directory over it.
WORKDIR /app

# No ENTRYPOINT: run-build.mjs supplies the whole command so the prompt can go
# in on stdin exactly as it does on the host.
