# The image a generated app is built in.
#
# The generated app must not see the harness, grader, or test definitions.
#
# No harness, grader, or test definition is copied in or mounted at run time.
# An adapter can mount only its selected stack artifacts, read-only.
# Keep the readable tag, but bind the base to an exact manifest. Campaigns use
# the digest of the completed image, so every attempt runs the same artifact.
FROM node:22-slim@sha256:f86be15afa9a8277608e141ce2a8aa55d3d9c40845921b8511f4fb7897be2554

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
      git curl ca-certificates procps lsof chromium util-linux \
    && rm -rf /var/lib/apt/lists/*

# Browser testing uses the system browser; no downloads into the agent home are needed.
ENV CHROME_BIN=/usr/bin/chromium
COPY browser-tools/package*.json /opt/browser-tools/
RUN npm ci --prefix /opt/browser-tools --omit=dev --ignore-scripts \
    && node -e "require('/opt/browser-tools/node_modules/puppeteer-core')"

# Pinned, and the auto-updater disabled: a CLI that updates itself mid-series
# changes the thing under test between one backend and the next. Override at
# build time to move deliberately rather than by drift.
ARG CLAUDE_VERSION=2.1.284
ARG CODEX_VERSION=0.159.3
ENV DISABLE_AUTOUPDATER=1
RUN npm install -g @anthropic-ai/claude-code@${CLAUDE_VERSION} \
    && claude --version
RUN npm install -g @openai/codex@${CODEX_VERSION} && codex --version
# xAI's installer does not verify the download, so the binary is checked here.
ARG GROK_VERSION=1.0.46
ARG GROK_SHA256=41626a53292324140b92556b9d42ff5542e3dcd04aff85eafb8689dd4adb44fc
ENV GROK_DISABLE_AUTOUPDATER=1
RUN curl -fsSL "https://x.ai/cli/grok-${GROK_VERSION}-linux-x86_64.gz" | gzip -dc > /usr/local/bin/grok \
    && echo "${GROK_SHA256}  /usr/local/bin/grok" | sha256sum -c - \
    && chmod 0755 /usr/local/bin/grok && grok --version
# The release Google's installer resolves for linux_x64, checked against its manifest's SHA-512.
ARG AGY_URL=https://storage.googleapis.com/antigravity-public/antigravity-cli/1.3.2-5813501495738368/linux-x64/cli_linux_x64.tar.gz
ARG AGY_SHA512=8dc2bb8471cfa6326e29e2896add767ad2d1f3714304b70fbae720d68af011182e7afde43964a7a3f54c604ab72c4fad772c8595608b059cfc582fb6798a25d2
RUN curl -fsSL "${AGY_URL}" -o /tmp/agy.tar.gz \
    && echo "${AGY_SHA512}  /tmp/agy.tar.gz" | sha512sum -c - \
    && tar -xzf /tmp/agy.tar.gz -C /tmp antigravity && install -m 0755 /tmp/antigravity /usr/local/bin/agy \
    && rm /tmp/agy.tar.gz /tmp/antigravity && agy --version

# The coding session can change the app and its own temporary state. It does
# not run as root, so it cannot change the system or harness control files.
RUN useradd --uid 10001 --create-home --shell /bin/bash developer \
    && chmod 0700 /home/developer

# The app under construction. Everything the build writes lives here, and the
# host mounts its own work directory over it.
WORKDIR /app

# No ENTRYPOINT: the run-build command supplies the whole command so the prompt can go
# in on stdin exactly as it does on the host.
