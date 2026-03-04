---
title: Azure Self-Hosted VMs + Key Rotation & Key Vault
slug: /how-to/self-hosted-key-rotation
---

This guide explains how JWT signing key rotation works in self-hosted SpacetimeDB and how to avoid breaking `spacetime publish` during rotation.

## Assumptions and Risk Note

This guide assumes the following baseline:

- You operate 3 Azure VMs for hosted environments: `prod`, `test`, and `dev`.
- You may also run local development (`local`) outside those hosted VMs.
- You have current system-disk backups/snapshots before rotating keys or migrating data.

This guide shows one practical pattern that combines:

- Azure Key Vault for JWT key rotation and policy verification.
- `rsync` between hosts (`prod` -> `test` -> `dev`) for staged data migration/testing.

This pattern can be used in production, and many teams do so, but you should fully understand the operational risks before adopting it:

- key distribution and mount consistency across hosts,
- identity ownership constraints during publish,
- data consistency and rollback during host-to-host migration.

## Opinionated Setup (Reproducible Defaults)

This guide uses one strict path contract end-to-end:

- host source directory: `./.generated/spacetimedb-keys`
- host key files:
  - `./.generated/spacetimedb-keys/id_ecdsa`
  - `./.generated/spacetimedb-keys/id_ecdsa.pub`
- runtime mount target: `/etc/spacetimedb`
- runtime key files:
  - `/etc/spacetimedb/id_ecdsa`
  - `/etc/spacetimedb/id_ecdsa.pub`

Environment mapping for this guide:

- `prod` -> `azvmprod`
- `test` -> `azvmtest`
- `dev` -> `azvmdev`
- optional `local` for local workstation workflows

Operational defaults in this guide:

- run rotation with `just kv-rotate-jwt-keys ENV=<prod|test|dev|local>`,
- keep token-preservation enabled,
- keep mounted-key sync enabled (writes to `./.generated/spacetimedb-keys`),
- mount `./.generated/spacetimedb-keys` read-only into `/etc/spacetimedb`,
- use one tooling surface (single script is fine) that supports rotate, verify, and token re-sign workflows.

## Quickstart Scaffold

Use this first if you want a fast bootstrap in a new repo.

### 1) Generate a compatible keypair

```sh
mkdir -p ./.generated/spacetimedb-keys
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:prime256v1 -out ./.generated/spacetimedb-keys/id_ecdsa
openssl pkey -in ./.generated/spacetimedb-keys/id_ecdsa -pubout -out ./.generated/spacetimedb-keys/id_ecdsa.pub
chmod 600 ./.generated/spacetimedb-keys/id_ecdsa
chmod 644 ./.generated/spacetimedb-keys/id_ecdsa.pub
```

### 2) Start SpacetimeDB with explicit key paths

This command assumes `./.generated/spacetimedb-keys` is mounted into `/etc/spacetimedb` for the running process.

```sh
spacetime start \
  --listen-addr 0.0.0.0:3003 \
  --pg-port 5432 \
  --jwt-priv-key-path /etc/spacetimedb/id_ecdsa \
  --jwt-pub-key-path /etc/spacetimedb/id_ecdsa.pub
```

### 3) Run your rotation pipeline

```sh
just az-login
just kv-rotate-jwt-keys-preview
just kv-rotate-jwt-keys ENV=local
just kv-verify-jwt-keys
```

By default, the rotation tool preserves publisher identity tokens and refreshes mounted runtime key files unless you explicitly opt out.

### 4) Restart and validate

```sh
# Use your deploy tool (docker compose, kubectl, systemd, etc.)
<restart-or-redeploy-command>

# Check logs for your deployment markers
<log-command> | rg "PUBLISH_SUCCESS|PUBLISH_FAILED|InvalidSignature|not authorized"
```

## Deployment Modes

Choose one implementation track and keep the operation order identical:

1. `az-login`
2. preview rotation
3. apply rotation
4. sync mounted runtime keys
5. verify policy/parity
6. restart/redeploy
7. self-publish module
8. check markers/logs

### Mode A: Non-container self-hosted

Run directly on host with `spacetime start` and local key paths.

### Mode B: Docker self-hosted

Run in container, mount key files into `/etc/spacetimedb`, and pass explicit JWT key args.

## Background

SpacetimeDB signs local identity tokens with an EC keypair (ES256, P-256). The keypair is read from:

- `--jwt-priv-key-path` and `--jwt-pub-key-path` CLI args, or
- `[certificate-authority]` in `config.toml` (see [Standalone Configuration](../00200-reference/00100-cli-reference/00200-standalone-config.md)).

To generate compatible keys:

```sh
KEY_DIR="./.generated/spacetimedb-keys"
mkdir -p "${KEY_DIR}"
openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:prime256v1 -out "${KEY_DIR}/id_ecdsa"
openssl pkey -in "${KEY_DIR}/id_ecdsa" -pubout -out "${KEY_DIR}/id_ecdsa.pub"
chmod 600 "${KEY_DIR}/id_ecdsa"
chmod 644 "${KEY_DIR}/id_ecdsa.pub"
```

Mount this directory into `/etc/spacetimedb` in your runtime so startup paths remain:

```sh
docker run --rm \
  -v "$(pwd)/.generated/spacetimedb-keys:/etc/spacetimedb:ro" \
  clockworklabs/spacetime:v2.0.1 \
  start --jwt-priv-key-path=/etc/spacetimedb/id_ecdsa --jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub
```

Important constraints:

- Keys are loaded at server startup; there is no hot-reload for JWT keys.
- For locally issued tokens, SpacetimeDB validates against one active public key.
- Rotating keys invalidates tokens signed by the previous private key.

## Why rotation can cause 401 and 403

After rotation, there are two common failure modes:

- **401 Unauthorized**: the token signature is invalid for the new keypair.
- **403 Forbidden**: token is valid, but the caller identity is not the database owner.

The 403 case is the subtle one:

1. Database ownership is tied to the identity that first created/published the database.
2. If you clear CLI credentials and get a fresh local token, the new token usually has a different subject claim.
3. A different subject produces a different identity.
4. Publish/update now fails as "not authorized to perform action ... update database."

## Rotation strategies

### Strategy A: Clean-slate rotation (dev/CI/stateless)

Use this when you are fine recreating databases and losing existing data.

1. Rotate keypair in your secret store.
2. Restart SpacetimeDB so it loads new keys.
3. Clear persisted Spacetime CLI config used by your publish step.
4. Re-publish, which creates a fresh owner identity and database state.

Tradeoff: this resets ownership and data.

### Strategy B: Identity-preserving rotation (stateful)

Use this when you must keep the existing database owner identity.

1. Before rotation, capture the current owner token (or at minimum its `sub` and `iss` claims).
2. Rotate keypair.
3. Mint a new JWT signed by the new private key but with the same `iss` and `sub`.
4. Update the CLI token used by `spacetime publish`.
5. Restart SpacetimeDB and publish.

This preserves identity because identity derivation depends on claims (`iss`/`sub`), not on which key signed the token.
In practice, keep these defaults unless you have a break-glass reason:

- default token path: `./data/.config/spacetime/cli.toml`,
- environment-aware preserve mode (`prod|test|dev|local`),
- mounted-key sync on by default.

Tradeoff: more operational complexity; requires careful token handling.

### Strategy C: OIDC-backed identity (recommended for production)

For production systems, prefer [Authentication](../../00200-core-concepts/00500-authentication.md) via OIDC providers. In that model:

- your external IdP controls user/service identity (`iss` + `sub`),
- SpacetimeDB local signing-key rotation does not redefine those identities.

## Recommended operational sequence

If you are using vault-managed keys and containerized deploys:

1. Preview rotation (dry-run) in your automation.
2. Rotate keypair secrets.
3. Refresh mounted runtime key files used by your service startup (for example `id_ecdsa` and `id_ecdsa.pub`).
4. Verify private/public pair parity.
5. Restart SpacetimeDB deployment so new key files are used.
6. Validate startup and publish logs.
7. If needed, clear or replace persisted CLI credentials based on chosen strategy.

## Azure Key Vault example (self-hosted)

This example uses generic `just` recipes and names so you can adapt it to your own repo.

### Example command flow

```sh
# 1) Authenticate to Azure
just az-login

# 2) Preview rotation (no writes)
just kv-rotate-jwt-keys-preview

# 3) Apply rotation (choose env per host)
just kv-rotate-jwt-keys ENV=local
# just kv-rotate-jwt-keys ENV=dev
# just kv-rotate-jwt-keys ENV=test
# just kv-rotate-jwt-keys ENV=prod

# 4) Verify keypair parity and policy
just kv-verify-jwt-keys
```

### Direct Azure CLI pull/push snippets

Use these when you want explicit Key Vault I/O commands in addition to the higher-level `just` wrappers.

```sh
#!/usr/bin/env bash
set -euo pipefail

VAULT_NAME="kv-local" # or kv-dev, kv-test, kv-prod
SECRET_PRIVATE="spacetimedb-jwt-private-key"
SECRET_PUBLIC="spacetimedb-jwt-public-key"
KEY_DIR="./.generated/spacetimedb-keys"

mkdir -p "${KEY_DIR}"

# Pull from Azure Key Vault -> local key files
az keyvault secret show --vault-name "${VAULT_NAME}" --name "${SECRET_PRIVATE}" --query value -o tsv > "${KEY_DIR}/id_ecdsa"
az keyvault secret show --vault-name "${VAULT_NAME}" --name "${SECRET_PUBLIC}" --query value -o tsv > "${KEY_DIR}/id_ecdsa.pub"
chmod 600 "${KEY_DIR}/id_ecdsa"
chmod 644 "${KEY_DIR}/id_ecdsa.pub"

# Push local key files -> Azure Key Vault
az keyvault secret set --vault-name "${VAULT_NAME}" --name "${SECRET_PRIVATE}" --value "$(cat "${KEY_DIR}/id_ecdsa")" --only-show-errors >/dev/null
az keyvault secret set --vault-name "${VAULT_NAME}" --name "${SECRET_PUBLIC}" --value "$(cat "${KEY_DIR}/id_ecdsa.pub")" --only-show-errors >/dev/null
```

After pull/push, run parity checks before restart/redeploy:

```sh
KEY_DIR="./.generated/spacetimedb-keys"
openssl pkey -in "${KEY_DIR}/id_ecdsa" -pubout -out /tmp/derived.pub
diff -u <(openssl pkey -pubin -in "${KEY_DIR}/id_ecdsa.pub" -outform PEM) <(openssl pkey -pubin -in /tmp/derived.pub -outform PEM)
```

### Example environment policy

- `kv-prod`: unique keypair
- `kv-test`: unique keypair
- `kv-dev` + `kv-local`: shared keypair

### Example secret names

- `spacetimedb-jwt-private-key`
- `spacetimedb-jwt-public-key`

### What your rotation tool should guarantee

- Generate ES256-compatible P-256 keypairs.
- Normalize PEM values before upload (`\r\n` -> `\n`, ensure trailing newline).
- Verify each private/public pair matches.
- Compute fingerprints and enforce your policy across environments.
- Require explicit confirmation for mutating operations.
- Support both dry-run and verify-only modes.
- Preserve publisher identity by default (re-sign cached CLI token), with explicit opt-out for break-glass cases.
- Refresh mounted key files by default, with explicit opt-out for advanced workflows.
- Support environment-aware token re-signing (`prod|test|dev|local`).
- Support override for cached token path (default can be `./data/.config/spacetime/cli.toml`).

## Scaffold Templates

These templates are intentionally generic so you can paste and adapt them in your own repository.

### Template: `justfile` recipes

```make
[group('azure')]
az-login:
    az login --use-device-code && az account show --query "{name:name, id:id}" -o table

[group('azure')]
kv-rotate-jwt-keys-preview:
    bun tools/azure/spacetimedb-tooling.ts --dry-run --verbose

[group('azure')]
[confirm("This rotates JWT signing keys in Key Vault environments. Continue?")]
kv-rotate-jwt-keys ENV="local":
    bun tools/azure/spacetimedb-tooling.ts --yes --preserve-publisher-token-env "{{ENV}}"

[group('azure')]
kv-verify-jwt-keys:
    bun tools/azure/spacetimedb-tooling.ts --verify-only

[group('azure')]
kv-get SECRET VAULT="kv-local":
    @az keyvault secret show --vault-name "{{VAULT}}" --name "{{SECRET}}" --query 'value' -o tsv

[group('azure')]
kv-set SECRET VALUE VAULT="kv-local":
    az keyvault secret set --vault-name "{{VAULT}}" --name "{{SECRET}}" --value "{{VALUE}}" --only-show-errors

[group('azure')]
kv-resign-jwt-token CLI_TOML="./data/.config/spacetime/cli.toml" PRIV_KEY=".generated/spacetimedb-keys/id_ecdsa":
    bun tools/azure/spacetimedb-tooling.ts --resign-token-only --publisher-cli-toml-path "{{CLI_TOML}}" --private-key-path "{{PRIV_KEY}}"

[group('deploy')]
spacetimedb-restart:
    # replace with your orchestrator command
    <restart-or-redeploy-command>

[group('deploy')]
spacetimedb-publish:
    spacetime publish --yes --server http://127.0.0.1:3003 --js-path ./dist/bundle.js spacetime
```

### Template: rotation script skeleton (`tools/azure/spacetimedb-tooling.ts`)

```ts
#!/usr/bin/env bun
import { $ } from "bun";
import { chmod, mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { parseArgs } from "node:util";

const SECRET_PRIVATE = "spacetimedb-jwt-private-key";
const SECRET_PUBLIC = "spacetimedb-jwt-public-key";
const VAULTS = { prod: "kv-prod", test: "kv-test", dev: "kv-dev", local: "kv-local" } as const;
const { values } = parseArgs({
  args: Bun.argv.slice(2),
  options: {
    "dry-run": { type: "boolean", short: "d", default: false },
    "verify-only": { type: "boolean", default: false },
    "resign-token-only": { type: "boolean", default: false },
    "preserve-publisher-token": { type: "boolean", default: true },
    "no-preserve-publisher-token": { type: "boolean", default: false },
    "preserve-publisher-token-env": { type: "string", default: "local" },
    "publisher-cli-toml-path": { type: "string", default: "./data/.config/spacetime/cli.toml" },
    "private-key-path": { type: "string" },
    "sync-mounted-keys": { type: "boolean", default: true },
    "no-sync-mounted-keys": { type: "boolean", default: false },
    "mounted-keys-dir": { type: "string", default: ".generated/spacetimedb-keys" },
    yes: { type: "boolean", default: false },
    verbose: { type: "boolean", short: "v", default: false },
  },
});

const preservePublisherToken = values["preserve-publisher-token"] && !values["no-preserve-publisher-token"];
const syncMountedKeys = values["sync-mounted-keys"] && !values["no-sync-mounted-keys"];
const isResignOnly = values["resign-token-only"];

async function generateKeyPair(label: string, dir: string) {
  const priv = join(dir, `${label}_private.pem`);
  const pub = join(dir, `${label}_public.pem`);
  await $`openssl genpkey -algorithm EC -pkeyopt ec_paramgen_curve:prime256v1 -out ${priv}`.quiet();
  await $`openssl pkey -in ${priv} -pubout -out ${pub}`.quiet();
  await chmod(priv, 0o600);
  await chmod(pub, 0o644);
  return { privPem: await Bun.file(priv).text(), pubPem: await Bun.file(pub).text(), privPath: priv, pubPath: pub };
}

async function setSecret(vault: string, name: string, value: string) {
  await $`az keyvault secret set --vault-name ${vault} --name ${name} --value ${value} --only-show-errors`.quiet();
}

function normalizePem(value: string) {
  const unix = value.replace(/\r\n/g, "\n").replace(/\r/g, "\n");
  return unix.endsWith("\n") ? unix : `${unix}\n`;
}

async function fingerprintPublicKey(pubPath: string) {
  const out = await $`bash -lc ${`openssl pkey -pubin -in "${pubPath}" -outform DER | openssl dgst -sha256`}`.text();
  return out.trim();
}

async function preserveLocalPublisherToken(cliTomlPath: string, privatePem: string) {
  // decode existing token, keep iss/sub/aud/hex_identity, refresh iat, clear exp, re-sign with ES256
}

async function syncMountedLocalKeys(targetDir: string, privatePem: string, publicPem: string) {
  await mkdir(targetDir, { recursive: true });
  await Bun.write(join(targetDir, "id_ecdsa"), privatePem);
  await Bun.write(join(targetDir, "id_ecdsa.pub"), publicPem);
  await chmod(join(targetDir, "id_ecdsa"), 0o600);
  await chmod(join(targetDir, "id_ecdsa.pub"), 0o644);
}

async function runResignOnly(cliTomlPath: string, privateKeyPath: string) {
  const privatePem = await Bun.file(privateKeyPath).text();
  await preserveLocalPublisherToken(cliTomlPath, privatePem);
}

// flow:
// 1) require --yes unless dry-run/verify-only
// 2) generate prod/test/shared keypairs
// 3) normalize PEM values before upload
// 4) upload by policy (prod unique, test unique, dev/local shared)
// 5) preserve token identity by default (unless no-preserve flag)
// 6) sync mounted keys by default (unless no-sync flag)
// 7) support re-sign-only mode for post-rsync destination identity continuity
// 8) verify parity + fingerprints + policy, exit non-zero on failure
```

### Template: entrypoint publish flow

```sh
#!/bin/sh
set -eu

DATA_DIR="${DATA_DIR:-/var/lib/spacetime/data}"
CONFIG_HOME="${CONFIG_HOME:-${DATA_DIR}/.config}"
SERVER_URL="${SERVER_URL:-http://127.0.0.1:3003}"
DB_NAME="${DB_NAME:-spacetime}"
MAX_READY_ATTEMPTS="${MAX_READY_ATTEMPTS:-20}"
MAX_PUBLISH_ATTEMPTS="${MAX_PUBLISH_ATTEMPTS:-30}"
PUBLISH_RETRY_SECONDS="${PUBLISH_RETRY_SECONDS:-2}"
SUCCESS_MARKER="PUBLISH_SUCCESS"
FAILURE_MARKER="PUBLISH_FAILED"

mkdir -p "$DATA_DIR" "$CONFIG_HOME"
export XDG_CONFIG_HOME="$CONFIG_HOME"

spacetime start \
  --listen-addr 0.0.0.0:3003 \
  --pg-port 5432 \
  --jwt-priv-key-path=/etc/spacetimedb/id_ecdsa \
  --jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub &
SERVER_PID=$!

cleanup() { kill "$SERVER_PID" 2>/dev/null || true; }
trap cleanup INT TERM

ready_attempt=1
while [ "$ready_attempt" -le "$MAX_READY_ATTEMPTS" ]; do
  if curl -s --max-time 1 "${SERVER_URL}/v1/identity" >/dev/null 2>&1; then
    break
  fi
  if [ "$ready_attempt" -eq "$MAX_READY_ATTEMPTS" ]; then
    echo "${FAILURE_MARKER} server_not_ready"
    exit 1
  fi
  ready_attempt=$((ready_attempt + 1))
  sleep 1
done

publish_attempt=1
while [ "$publish_attempt" -le "$MAX_PUBLISH_ATTEMPTS" ]; do
  set +e
  OUT="$(spacetime publish --yes --server "$SERVER_URL" --js-path /app/dist/bundle.js "$DB_NAME" 2>&1)"
  STATUS=$?
  set -e
  [ -n "$OUT" ] && echo "$OUT"

  if [ "$STATUS" -eq 0 ]; then
    echo "${SUCCESS_MARKER} database=${DB_NAME} attempt=${publish_attempt}"
    break
  fi
  if echo "$OUT" | rg -q "403 Forbidden|not authorized to perform action"; then
    echo "${FAILURE_MARKER} publish_unauthorized database=${DB_NAME}"
    exit 1
  fi
  if [ "$publish_attempt" -eq "$MAX_PUBLISH_ATTEMPTS" ]; then
    echo "${FAILURE_MARKER} publish_retries_exhausted database=${DB_NAME}"
    exit 1
  fi
  publish_attempt=$((publish_attempt + 1))
  sleep "$PUBLISH_RETRY_SECONDS"
done

wait "$SERVER_PID"
```

### Template: Docker Compose key mount

```yaml
services:
  spacetimedb:
    image: your-org/spacetimedb:latest
    volumes:
      - ./.generated/spacetimedb-keys:/etc/spacetimedb:ro
    command:
      - start
      - --listen-addr=0.0.0.0:3003
      - --pg-port=5432
      - --jwt-priv-key-path=/etc/spacetimedb/id_ecdsa
      - --jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub
```

## Docker and entrypoint considerations

If your container expects:

- `/etc/spacetimedb/id_ecdsa`
- `/etc/spacetimedb/id_ecdsa.pub`

make sure those files are mounted before process startup.

Generic runtime flow:

1. Read private/public PEM values from Azure Key Vault.
2. Write them to `./.generated/spacetimedb-keys/id_ecdsa` and `./.generated/spacetimedb-keys/id_ecdsa.pub`.
3. Mount the directory read-only to `/etc/spacetimedb`.
4. Start SpacetimeDB with:

```sh
spacetime start \
  --jwt-priv-key-path=/etc/spacetimedb/id_ecdsa \
  --jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub
```

If neither key file exists at startup, SpacetimeDB may generate keys locally, which can diverge from your managed secrets and cause auth mismatches across instances.

For broader deployment guidance, see [Self-hosting](./00100-deploy/00200-self-hosting.md).

If your rotation automation supports mounted-key sync, keep it enabled by default so the mounted files are refreshed in the same run as secret rotation.

### Deployment variants

Use the same key material and startup args, then adapt only the mounting strategy:

#### Kubernetes secret volume

```yaml
apiVersion: v1
kind: Secret
metadata:
  name: spacetimedb-jwt-keys
type: Opaque
stringData:
  id_ecdsa: |
    -----BEGIN PRIVATE KEY-----
    <private-key>
    -----END PRIVATE KEY-----
  id_ecdsa.pub: |
    -----BEGIN PUBLIC KEY-----
    <public-key>
    -----END PUBLIC KEY-----
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: spacetimedb
spec:
  template:
    spec:
      volumes:
        - name: jwt-keys
          secret:
            secretName: spacetimedb-jwt-keys
      containers:
        - name: spacetimedb
          image: your-org/spacetimedb:latest
          volumeMounts:
            - name: jwt-keys
              mountPath: /etc/spacetimedb
              readOnly: true
```

#### systemd service

```ini
[Service]
ExecStart=/usr/local/bin/spacetime start \
  --listen-addr=0.0.0.0:3003 \
  --pg-port=5432 \
  --jwt-priv-key-path=/etc/spacetimedb/id_ecdsa \
  --jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub
Restart=always
```

## Operational runbook (copy/paste)

```sh
# 0) Azure auth (run where your just/az tooling lives)
just az-login

# Preview
just kv-rotate-jwt-keys-preview

# Rotate (run on each host with matching env)
# run on azvmprod
just kv-rotate-jwt-keys ENV=prod
# run on azvmtest
just kv-rotate-jwt-keys ENV=test
# run on azvmdev
just kv-rotate-jwt-keys ENV=dev
# optional local workstation
just kv-rotate-jwt-keys ENV=local

# Verify
just kv-verify-jwt-keys

# Optional staged data migration path (execute re-sign on destination host context)
rsync -aHAX --delete user@azvmprod:/var/lib/spacetime/data/ user@azvmtest:/var/lib/spacetime/data/
ssh azvmtest 'cd <repo-root> && bun tools/azure/spacetimedb-tooling.ts --resign-token-only --publisher-cli-toml-path "./data/.config/spacetime/cli.toml" --private-key-path "./.generated/spacetimedb-keys/id_ecdsa"'
rsync -aHAX --delete user@azvmtest:/var/lib/spacetime/data/ user@azvmdev:/var/lib/spacetime/data/
ssh azvmdev 'cd <repo-root> && bun tools/azure/spacetimedb-tooling.ts --resign-token-only --publisher-cli-toml-path "./data/.config/spacetime/cli.toml" --private-key-path "./.generated/spacetimedb-keys/id_ecdsa"'

# Restart/redeploy so rotated keys are loaded (run on destination host)
just spacetimedb-restart

# Self-publish module to your self-hosted instance
just spacetimedb-publish

# Check runtime markers
<log-command> | rg "PUBLISH_SUCCESS|PUBLISH_FAILED|InvalidSignature|not authorized"
```

After restart/redeploy, verify your logs include your expected publish marker, for example:

- `PUBLISH_SUCCESS`
- `PUBLISH_FAILED`

### Data sync variant (prod -> test -> dev)

Use this when you are promoting data snapshots across environments and need publish identity continuity on each destination host.

What is implemented vs conceptual:

- The reference tooling demonstrates concrete sync flows such as `prod -> test` and host-local pulls.
- The `prod -> test -> dev` sequence below is a topology pattern built from those same primitives.

```sh
# 1) Preview (run from control host)
rsync -aHAX --delete --dry-run user@azvmprod:/var/lib/spacetime/data/ user@azvmtest:/var/lib/spacetime/data/

# 2) Apply prod -> test (run from control host)
rsync -aHAX --delete user@azvmprod:/var/lib/spacetime/data/ user@azvmtest:/var/lib/spacetime/data/

# 3) Re-sign on test (run on azvmtest or via ssh)
ssh azvmtest 'cd <repo-root> && bun tools/azure/spacetimedb-tooling.ts \
  --resign-token-only \
  --publisher-cli-toml-path "./data/.config/spacetime/cli.toml" \
  --private-key-path "./.generated/spacetimedb-keys/id_ecdsa"'

# 4) Repeat test -> dev (run from control host)
rsync -aHAX --delete user@azvmtest:/var/lib/spacetime/data/ user@azvmdev:/var/lib/spacetime/data/
ssh azvmdev 'cd <repo-root> && bun tools/azure/spacetimedb-tooling.ts \
  --resign-token-only \
  --publisher-cli-toml-path "./data/.config/spacetime/cli.toml" \
  --private-key-path "./.generated/spacetimedb-keys/id_ecdsa"'

# 5) Restart + publish checks (run on destination host after each hop)
just spacetimedb-restart
just spacetimedb-publish
<log-command> | rg "PUBLISH_SUCCESS|PUBLISH_FAILED|not authorized|InvalidSignature"
```

Operational guardrails:

- `rsync --delete` is destructive; verify backups/snapshots before apply.
- Stop service before sync and restart after sync on destination to avoid partial-state reads.
- Treat key parity check + destination re-sign + publish marker validation as required promotion gates.

Detailed host-scoped sync pattern (modeled after production recipes):

```sh
# run on destination host (example: azvmtest) before pull/apply
<stop-service-command>

# run on control host: preview first
rsync -aHAX --delete --dry-run user@azvmprod:/var/lib/spacetime/data/ user@azvmtest:/var/lib/spacetime/data/

# run on control host: apply
rsync -aHAX --delete user@azvmprod:/var/lib/spacetime/data/ user@azvmtest:/var/lib/spacetime/data/

# run on destination host context (directly or via ssh): re-sign local cached token
ssh azvmtest 'cd <repo-root> && bun tools/azure/spacetimedb-tooling.ts --resign-token-only --publisher-cli-toml-path "./data/.config/spacetime/cli.toml" --private-key-path "./.generated/spacetimedb-keys/id_ecdsa"'

# run on destination host: start + verify publish markers
<start-service-command>
just spacetimedb-publish
<log-command> | rg "PUBLISH_SUCCESS|PUBLISH_FAILED|InvalidSignature|not authorized"
```

## Reproducibility Addendum (Operator Copy/Paste)

Use these snippets to reduce setup drift and make rotation/sync runs repeatable across teams.

### 1) Preflight checks (before rotate or sync)

```sh
#!/usr/bin/env bash
set -euo pipefail

REQUIRED_BINS=(az openssl bun rsync spacetime ssh)
KEY_DIR="./.generated/spacetimedb-keys"
CLI_TOML="./data/.config/spacetime/cli.toml"

for bin in "${REQUIRED_BINS[@]}"; do
  command -v "${bin}" >/dev/null 2>&1 || { echo "missing dependency: ${bin}"; exit 1; }
done

az account show >/dev/null 2>&1 || { echo "azure login required: run just az-login"; exit 1; }

mkdir -p "${KEY_DIR}"
[ -f "${KEY_DIR}/id_ecdsa" ] || echo "note: ${KEY_DIR}/id_ecdsa not present yet (expected before first pull/rotation)"
[ -f "${KEY_DIR}/id_ecdsa.pub" ] || echo "note: ${KEY_DIR}/id_ecdsa.pub not present yet (expected before first pull/rotation)"
[ -f "${CLI_TOML}" ] || echo "note: ${CLI_TOML} not present yet (expected before first publish/token write)"

echo "preflight ok"
```

### 2) Safer AKV pull/push with files

Prefer `--file` for upload to avoid placing multiline PEM values in shell command arguments.

```sh
#!/usr/bin/env bash
set -euo pipefail

VAULT_NAME="kv-local" # or kv-dev, kv-test, kv-prod
SECRET_PRIVATE="spacetimedb-jwt-private-key"
SECRET_PUBLIC="spacetimedb-jwt-public-key"
KEY_DIR="./.generated/spacetimedb-keys"

mkdir -p "${KEY_DIR}"

# Pull: Key Vault -> local files
az keyvault secret show --vault-name "${VAULT_NAME}" --name "${SECRET_PRIVATE}" --query value -o tsv > "${KEY_DIR}/id_ecdsa"
az keyvault secret show --vault-name "${VAULT_NAME}" --name "${SECRET_PUBLIC}" --query value -o tsv > "${KEY_DIR}/id_ecdsa.pub"
chmod 600 "${KEY_DIR}/id_ecdsa"
chmod 644 "${KEY_DIR}/id_ecdsa.pub"

# Push: local files -> Key Vault
az keyvault secret set --vault-name "${VAULT_NAME}" --name "${SECRET_PRIVATE}" --file "${KEY_DIR}/id_ecdsa" --encoding utf-8 --only-show-errors >/dev/null
az keyvault secret set --vault-name "${VAULT_NAME}" --name "${SECRET_PUBLIC}" --file "${KEY_DIR}/id_ecdsa.pub" --encoding utf-8 --only-show-errors >/dev/null
```

### 3) Single-host end-to-end rotation script

```sh
#!/usr/bin/env bash
set -euo pipefail

ENVIRONMENT="${ENVIRONMENT:-local}" # prod|test|dev|local

just az-login
just kv-rotate-jwt-keys-preview
just kv-rotate-jwt-keys ENV="${ENVIRONMENT}"
just kv-verify-jwt-keys

# Load rotated keys in runtime
<restart-or-redeploy-command>

# Publish validation gate
just spacetimedb-publish
<log-command> | rg "PUBLISH_SUCCESS|PUBLISH_FAILED|InvalidSignature|not authorized"
```

### 4) Identity continuity assertion (before/after re-sign)

Run once before `--resign-token-only` and once after; `iss` and `sub` should remain stable.

```sh
bun - <<'TS'
import { readFileSync } from "node:fs";

const cliTomlPath = "./data/.config/spacetime/cli.toml";
const text = readFileSync(cliTomlPath, "utf8");
const match = text.match(/^\s*spacetimedb_token\s*=\s*"([^"]+)"\s*$/m);
if (!match?.[1]) {
  throw new Error("spacetimedb_token not found");
}

const token = match[1];
const parts = token.split(".");
if (parts.length !== 3 || !parts[1]) {
  throw new Error("invalid token format");
}

const payloadJson = Buffer.from(parts[1], "base64url").toString("utf8");
const claims = JSON.parse(payloadJson) as Record<string, unknown>;

console.log(
  JSON.stringify(
    {
      iss: claims.iss ?? null,
      sub: claims.sub ?? null,
      aud: claims.aud ?? null,
      hex_identity: claims.hex_identity ?? null,
    },
    null,
    2,
  ),
);
TS
```

### 5) SSH host alias template for rsync workflows

```sshconfig
Host azvmprod
  HostName <prod-hostname-or-ip>
  User <ssh-user>
  IdentityFile ~/.ssh/azvmsync

Host azvmtest
  HostName <test-hostname-or-ip>
  User <ssh-user>
  IdentityFile ~/.ssh/azvmsync

Host azvmdev
  HostName <dev-hostname-or-ip>
  User <ssh-user>
  IdentityFile ~/.ssh/azvmsync
```

### 6) Generate `azvmsync` key for inter-VM sync

Run these from your control host (the machine that runs `ssh`/`rsync` commands).

```sh
#!/usr/bin/env bash
set -euo pipefail

mkdir -p ~/.ssh
chmod 700 ~/.ssh

# Create a dedicated key for VM-to-VM/data-sync operations.
ssh-keygen -t ed25519 -f ~/.ssh/azvmsync -C "azvmsync" -N ""
chmod 600 ~/.ssh/azvmsync
chmod 644 ~/.ssh/azvmsync.pub
```

Install the public key on each destination host:

```sh
# Option A: if ssh-copy-id is available
for host in azvmprod azvmtest azvmdev; do
  ssh-copy-id -i ~/.ssh/azvmsync.pub "$host"
done

# Option B: manual append if ssh-copy-id is unavailable
PUBKEY="$(cat ~/.ssh/azvmsync.pub)"
for host in azvmprod azvmtest azvmdev; do
  ssh "$host" "mkdir -p ~/.ssh && chmod 700 ~/.ssh && grep -qxF '$PUBKEY' ~/.ssh/authorized_keys || echo '$PUBKEY' >> ~/.ssh/authorized_keys && chmod 600 ~/.ssh/authorized_keys"
done
```

Verify passwordless connectivity:

```sh
for host in azvmprod azvmtest azvmdev; do
  ssh -i ~/.ssh/azvmsync "$host" "hostname"
done
```

If you use host aliases from the prior section, your `rsync` commands can stay short and consistent.

## Self-Publish and Versioning

Keep module publish and runtime versions explicit to avoid drift across environments.

### Non-container version pinning

```sh
# Pin CLI/runtime version in your install process
spacetime version list
spacetime install 2.0.1
spacetime version use 2.0.1
```

### Container image versioning

```dockerfile
FROM clockworklabs/spacetime:v2.0.1
LABEL org.opencontainers.image.version="2.0.1"
```

```sh
# Build and push with immutable and rolling tags
docker build -t registry.example.com/spacetimedb-app:v2.0.1 -t registry.example.com/spacetimedb-app:v2.0 .
docker push registry.example.com/spacetimedb-app:v2.0.1
docker push registry.example.com/spacetimedb-app:v2.0
```

### Deployment checkpoints

- confirm module artifact exists before image build (for example `dist/bundle.js`),
- deploy pinned tag first, then optional rolling tag updates,
- verify publish markers after rollout,
- rollback by redeploying previous pinned image tag.

## Verification Commands

Use these commands directly in CI/CD or local troubleshooting.

### Validate PEM structure

```sh
KEY_DIR="./.generated/spacetimedb-keys"
openssl pkey -in "${KEY_DIR}/id_ecdsa" -noout
openssl pkey -pubin -in "${KEY_DIR}/id_ecdsa.pub" -noout
```

### Validate that public key matches private key

```sh
KEY_DIR="./.generated/spacetimedb-keys"
openssl pkey -in "${KEY_DIR}/id_ecdsa" -pubout -out /tmp/derived.pub
diff -u <(openssl pkey -pubin -in "${KEY_DIR}/id_ecdsa.pub" -outform PEM) <(openssl pkey -pubin -in /tmp/derived.pub -outform PEM)
```

### Print stable SHA-256 fingerprint

```sh
KEY_DIR="./.generated/spacetimedb-keys"
openssl pkey -pubin -in "${KEY_DIR}/id_ecdsa.pub" -outform DER | openssl dgst -sha256
```

### Normalize newline style before upload

```sh
# Converts CRLF to LF and ensures trailing newline
python3 - <<'PY'
from pathlib import Path
key_dir = Path(".generated/spacetimedb-keys")
for name in ["id_ecdsa", "id_ecdsa.pub"]:
    path = key_dir / name
    text = path.read_text()
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    if not text.endswith("\n"):
        text += "\n"
    Path(f"{path}.normalized").write_text(text)
PY
```

## AI/Automation Contract

Use this section as a machine-readable checklist for scripts or AI agents.

### Inputs

- Secret names:
  - `spacetimedb-jwt-private-key`
  - `spacetimedb-jwt-public-key`
- Vault environments:
  - `kv-prod`
  - `kv-test`
  - `kv-dev`
  - `kv-local`
- Policy:
  - prod/test unique keypairs
  - dev/local shared keypair

### Outputs

- Canonical path mapping:
  - host source: `./.generated/spacetimedb-keys/id_ecdsa` -> runtime: `/etc/spacetimedb/id_ecdsa`
  - host source: `./.generated/spacetimedb-keys/id_ecdsa.pub` -> runtime: `/etc/spacetimedb/id_ecdsa.pub`
- Startup args:
  - `--jwt-priv-key-path=/etc/spacetimedb/id_ecdsa`
  - `--jwt-pub-key-path=/etc/spacetimedb/id_ecdsa.pub`
- Identity-preserving token source:
  - default: `./data/.config/spacetime/cli.toml`
  - override: `--publisher-cli-toml-path <path>`

### Required command order

```sh
just az-login
just kv-rotate-jwt-keys-preview
just kv-rotate-jwt-keys ENV=<prod|test|dev|local>
just kv-verify-jwt-keys
rsync -aHAX --delete <src-host>:/var/lib/spacetime/data/ <dst-host>:/var/lib/spacetime/data/
ssh <dst-host> 'cd <repo-root> && bun tools/azure/spacetimedb-tooling.ts --resign-token-only --publisher-cli-toml-path <dst-cli-toml> --private-key-path <dst-private-key>'
<restart-or-redeploy-command>
```

### Success/failure markers

- Success marker example: `PUBLISH_SUCCESS`
- Failure marker example: `PUBLISH_FAILED`
- Auth signals to detect:
  - `InvalidSignature` (usually stale token/key mismatch)
  - `not authorized to perform action` (ownership mismatch)

### Non-goals

- No hot-reload for JWT keys (restart required)
- No automatic ownership transfer during rotation
- No graceful dual-signing window for local self-signed tokens

## Troubleshooting

```mermaid
flowchart TD
  startNode["Publish Or Connect Fails"] --> statusCheck{"HTTP Status?"}
  statusCheck -->|"401"| sigPath["Signature Path"]
  statusCheck -->|"403"| ownerPath["Ownership Path"]
  statusCheck -->|"other"| genericPath["General Runtime Checks"]

  sigPath --> sigStep1["Confirm active key files are mounted at /etc/spacetimedb"]
  sigStep1 --> sigStep2["Verify pair parity and SHA256 fingerprint"]
  sigStep2 --> sigStep3["Replace stale cached CLI token if needed"]
  sigStep3 --> sigStep4["Restart service to load new keys"]

  ownerPath --> ownerStep1["Token is valid, but caller identity is not DB owner"]
  ownerStep1 --> ownerStep2["Check XDG_CONFIG_HOME persistence and publish identity source"]
  ownerStep2 --> ownerStep3["Choose strategy: clean-slate or identity-preserving"]

  genericPath --> genStep1["Confirm restart/redeploy happened after rotation"]
  genericPath --> genStep2["Check markers: PUBLISH_SUCCESS or PUBLISH_FAILED"]
```

### `401` path: signature mismatch

Typical cause: token signed with an old/private key pair that no longer matches the server public key.

Checks:

- verify `id_ecdsa` and `id_ecdsa.pub` are the files actually mounted into `/etc/spacetimedb`,
- run the verification cookbook commands to check parity and fingerprints,
- refresh cached CLI token/config if it still carries old signature material,
- restart the service so the rotated files are loaded.

### `403` path: ownership mismatch

Typical cause: token is valid, but identity differs from the identity that owns the database.

Checks:

- confirm the publish step uses the expected persisted config path (`XDG_CONFIG_HOME`),
- confirm whether you intentionally rotated into a new identity (clean-slate) or need identity-preserving flow,
- if you cleared cached CLI auth/config, validate that your new token maps to the expected owner identity.

### General runtime checks

- Confirm rotation order: preview -> apply -> verify -> restart.
- Confirm mounts are read-only and at the expected path.
- Confirm publish step and log markers are emitted by the same runtime instance.
