# syntax=docker/dockerfile:1.7

FROM docker:29.6.2-cli@sha256:feb2d49bd65f274b3e4b4620beabe2f4691e5287e496da9fbc9830ed5f780676 AS docker-cli
FROM mcr.microsoft.com/playwright:v1.62.1-noble@sha256:c091b21d9fae78c76e85cd4356431e9b018402f172a214fc7d7a5e9a7e29d8ac AS source
SHELL ["/bin/bash", "-o", "pipefail", "-c"]
RUN apt-get update && apt-get install -y --no-install-recommends lsof=4.95.0-1build3 util-linux \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /workspace
# Git metadata is mounted only for trusted source export/verification. It is never
# copied into an image layer. Release builds require a clean normal Git checkout.
RUN --mount=type=bind,target=/checkout \
    git -c safe.directory=/checkout -C /checkout archive HEAD | tar -x -C /workspace
WORKDIR /workspace/tools/stack-bench
RUN npm ci --ignore-scripts --no-audit --no-fund && npm run build
# Normalize checkout text as Git does on Windows; explicit .gitattributes still apply.
RUN --mount=type=bind,target=/checkout \
    GIT_OPTIONAL_LOCKS=0 GIT_CONFIG_COUNT=2 GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0=/checkout \
    GIT_CONFIG_KEY_1=core.autocrlf GIT_CONFIG_VALUE_1=input \
    node --input-type=module -e 'import {writeFileSync} from "node:fs"; import {releaseSourceIdentity,binarySourceIdentity} from "./dist/src/releases/release-source.js"; writeFileSync("/workspace/stack-bench-source.json",JSON.stringify(releaseSourceIdentity("/checkout"))); writeFileSync("/workspace/stack-bench-binary-source.json",JSON.stringify(binarySourceIdentity("/checkout")));'

FROM rust:1.93-slim-bookworm@sha256:8f8609d448e821fbc0e44241bc5ca4ce49663cc6306ff1a17f655a0e2a7cd084 AS binary-build
RUN apt-get update -qq && apt-get install -y --no-install-recommends \
    pkg-config libssl-dev build-essential clang cmake perl git curl python3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /workspace
COPY --from=source /workspace/ ./
# The existing source-archive path in cli/build.rs avoids retaining Git metadata.
RUN --mount=type=cache,id=stack-bench-rust-target,target=/target \
    --mount=type=cache,id=stack-bench-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=stack-bench-cargo-git,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,id=stack-bench-rustup,target=/usr/local/rustup,sharing=locked \
    export SPACETIMEDB_NIX_BUILD_GIT_COMMIT="$(python3 -c 'import json; print(json.load(open("stack-bench-source.json"))["revision"])')" \
    && CARGO_TARGET_DIR=/target cargo build --release --locked \
      -p spacetimedb-cli --bin spacetimedb-cli \
      -p spacetimedb-standalone --bin spacetimedb-standalone \
    && mkdir -p /binaries \
    && cp /target/release/spacetimedb-cli /target/release/spacetimedb-standalone /binaries/

FROM source AS stack-bench-build
COPY --from=binary-build /binaries/ ./container/bin/
RUN node dist/container/binary-provenance.js record-snapshot \
      --root /workspace/tools/stack-bench --source-file /workspace/stack-bench-binary-source.json \
    && npm prune --omit=dev --ignore-scripts --no-audit --no-fund

# build-linux-cli.sh uses this same build, rather than a second Rust recipe.
FROM scratch AS binary-export
COPY --from=stack-bench-build /workspace/tools/stack-bench/container/bin/ /bin/
COPY --from=stack-bench-build /workspace/tools/stack-bench/container/spacetimedb-binaries.json /spacetimedb-binaries.json

FROM source AS sdk-build
WORKDIR /workspace
RUN corepack enable \
    && pnpm --filter spacetimedb install --frozen-lockfile --ignore-scripts \
    && pnpm --filter spacetimedb run build \
    && test -f crates/bindings-typescript/dist/server/index.d.ts \
    && test -f crates/bindings-typescript/dist/server/index.mjs

FROM mcr.microsoft.com/playwright:v1.62.1-noble@sha256:c091b21d9fae78c76e85cd4356431e9b018402f172a214fc7d7a5e9a7e29d8ac

ENV NODE_ENV=production \
    PLAYWRIGHT_BROWSERS_PATH=/ms-playwright

COPY --from=docker-cli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=docker-cli /usr/local/libexec/docker/cli-plugins /usr/local/libexec/docker/cli-plugins
ADD --checksum=sha256:4629c757b7618056f8ddd7e2625ae9fdd94c0372a65049520bc7d9df9efc7f71 \
    https://github.com/sigstore/cosign/releases/download/v3.1.3/cosign-linux-amd64 \
    /usr/local/bin/cosign

RUN apt-get update \
    && apt-get install -y --no-install-recommends lsof=4.95.0-1build3 nftables util-linux \
    && chmod 0555 /usr/local/bin/cosign \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/stack-bench
LABEL org.opencontainers.image.title="Stack Bench controller"

COPY --from=stack-bench-build /workspace/tools/stack-bench/ ./
COPY --from=stack-bench-build /workspace/stack-bench-source.json ./source-identity.json
RUN chmod 0555 /opt/stack-bench/dist/container/browser-pipe.js
COPY --from=source /workspace/skills/ /skills/
COPY --from=source /workspace/crates/bindings-typescript/ /opt/stack-bench-embedded-deps/bindings-typescript/
COPY --from=sdk-build /workspace/crates/bindings-typescript/dist/ /opt/stack-bench-embedded-deps/bindings-typescript/dist/
COPY --from=source /workspace/licenses/BSL.txt /opt/stack-bench-embedded-deps/BSL.txt

RUN node dist/container/binary-provenance.js verify \
      --root /opt/stack-bench --source-sha256 "$(node -p "require('./source-identity.json').binarySourceSha256")" \
    && test "$(node -p "require('playwright/package.json').version")" = "1.62.1" \
    && rm -rf tests dist/tests \
    && install -m 0555 container/bin/spacetimedb-cli \
      /opt/stack-bench-embedded-deps/spacetimedb-cli \
    && install -m 0555 container/bin/spacetimedb-standalone \
      /opt/stack-bench-embedded-deps/spacetimedb-standalone \
    && rm /opt/stack-bench-embedded-deps/bindings-typescript/LICENSE.txt \
    && mv /opt/stack-bench-embedded-deps/BSL.txt \
      /opt/stack-bench-embedded-deps/bindings-typescript/LICENSE.txt \
    && chmod 0444 /opt/stack-bench-embedded-deps/bindings-typescript/LICENSE.txt \
    && chmod 0555 /opt/stack-bench-embedded-deps/spacetimedb-cli \
      /opt/stack-bench-embedded-deps/spacetimedb-standalone \
    && cd /opt/stack-bench-embedded-deps/bindings-typescript \
    && pack_name="$(npm pack --pack-destination /opt/stack-bench-embedded-deps --silent)" \
    && mv "/opt/stack-bench-embedded-deps/$pack_name" /opt/stack-bench-embedded-deps/spacetimedb.tgz \
    && tar -tzf /opt/stack-bench-embedded-deps/spacetimedb.tgz | grep -Fxq package/dist/server/index.d.ts \
    && tar -tzf /opt/stack-bench-embedded-deps/spacetimedb.tgz | grep -Fxq package/dist/server/index.mjs \
    && cd /opt/stack-bench \
    && node dist/appliance/dependency-volume.js manifest \
      --source /opt/stack-bench-embedded-deps \
      --out /opt/stack-bench/dependency-manifest.json \
    && node dist/appliance/dependency-volume.js verify \
      --target /opt/stack-bench-embedded-deps \
      --manifest /opt/stack-bench/dependency-manifest.json \
    && rm -rf results .spacetime-data .loop-test

ENTRYPOINT ["node", "/opt/stack-bench/dist/appliance/controller.js"]
CMD ["--help"]
