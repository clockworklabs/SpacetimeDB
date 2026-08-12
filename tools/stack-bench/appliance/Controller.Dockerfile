# syntax=docker/dockerfile:1.7

FROM docker:29.6.2-cli@sha256:feb2d49bd65f274b3e4b4620beabe2f4691e5287e496da9fbc9830ed5f780676 AS docker-cli
FROM mcr.microsoft.com/playwright:v1.62.1-noble@sha256:c091b21d9fae78c76e85cd4356431e9b018402f172a214fc7d7a5e9a7e29d8ac

ENV NODE_ENV=production \
    PLAYWRIGHT_BROWSERS_PATH=/ms-playwright

COPY --from=docker-cli /usr/local/bin/docker /usr/local/bin/docker
COPY --from=docker-cli /usr/local/libexec/docker/cli-plugins /usr/local/libexec/docker/cli-plugins

RUN apt-get update \
    && apt-get install -y --no-install-recommends lsof=4.95.0-1build3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /opt/stack-bench
COPY tools/stack-bench/package.json tools/stack-bench/package-lock.json ./
RUN npm ci --omit=dev --ignore-scripts --no-audit --no-fund \
    && test "$(node -p "require('playwright/package.json').version")" = "1.62.1"

ARG SOURCE_REVISION
ARG SOURCE_SHA256
RUN printf '%s' "$SOURCE_REVISION" | grep -Eq '^[0-9a-f]{40}([0-9a-f]{24})?$' \
    && printf '%s' "$SOURCE_SHA256" | grep -Eq '^[0-9a-f]{64}$'
LABEL org.opencontainers.image.title="Stack Bench controller" \
      org.opencontainers.image.revision="$SOURCE_REVISION"
ENV STACK_BENCH_SOURCE_REVISION=$SOURCE_REVISION \
    STACK_BENCH_SOURCE_SHA256=$SOURCE_SHA256

COPY tools/stack-bench/ ./
COPY crates/bindings-typescript/ /opt/stack-bench-embedded-deps/bindings-typescript/
COPY licenses/BSL.txt /opt/stack-bench-embedded-deps/BSL.txt
COPY tools/stack-bench/container/bin/spacetimedb-cli /opt/stack-bench-embedded-deps/spacetimedb-cli
COPY tools/stack-bench/container/bin/spacetimedb-standalone /opt/stack-bench-embedded-deps/spacetimedb-standalone

RUN rm /opt/stack-bench-embedded-deps/bindings-typescript/LICENSE.txt \
    && mv /opt/stack-bench-embedded-deps/BSL.txt \
      /opt/stack-bench-embedded-deps/bindings-typescript/LICENSE.txt \
    && chmod 0444 /opt/stack-bench-embedded-deps/bindings-typescript/LICENSE.txt \
    && chmod 0555 /opt/stack-bench-embedded-deps/spacetimedb-cli \
      /opt/stack-bench-embedded-deps/spacetimedb-standalone \
    && node appliance/dependency-volume.mjs manifest \
      --source /opt/stack-bench-embedded-deps \
      --out /opt/stack-bench/dependency-manifest.json \
    && node appliance/dependency-volume.mjs verify \
      --target /opt/stack-bench-embedded-deps \
      --manifest /opt/stack-bench/dependency-manifest.json \
    && rm -rf results .spacetime-data .loop-test archive/pre-v1/results

ENTRYPOINT ["node", "/opt/stack-bench/appliance/controller.mjs"]
CMD ["--help"]
