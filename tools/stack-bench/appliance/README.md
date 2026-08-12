# Stack Bench appliance

This directory is the dedicated-runner packaging for Stack Bench. It is not a
general workstation install. The supported v1 runner is a disposable
Linux/amd64 machine with a local Docker Engine and the fixed persistent path
`/var/lib/stack-bench`.

The controller has the Docker socket, which is root-equivalent access to the
runner. Use a machine that contains no unrelated workloads or credentials and
destroy it after copying verified results elsewhere.

## What is packaged

- `Controller.Dockerfile` builds the grader/controller with Playwright, a
  digest-pinned Docker CLI, and checksum-pinned Cosign 3.1.3 for qualified
  release verification.
- `dependency-volume.mjs` copies the release's exact SDK, CLI, and standalone
  runtime into one checksummed named volume. It refuses unmarked, changed, or
  wrong-release content.
- `docker-compose.yaml` starts the controller, one-shot dependency initializer,
  and digest-pinned PostgreSQL and MongoDB services.
- `secrets.env.example` documents the three operator-supplied values. It never
  contains a real secret or a usable mutable image tag.

The coding container receives the selected app, its own transcript directory,
and only the dependency paths declared by its stack adapter. It never receives
the controller image filesystem, scenarios, grader, results, or release
manifest.

## Build a development candidate

From a clean repository checkout, compute the exact tracked release-source
identity and build Linux/amd64:

```powershell
$source = node tools/stack-bench/release-source.mjs --json | ConvertFrom-Json
docker build --platform linux/amd64 `
  -f tools/stack-bench/appliance/Controller.Dockerfile `
  --build-arg SOURCE_REVISION=$($source.revision) `
  --build-arg SOURCE_SHA256=$($source.sha256) `
  -t stack-bench-controller:development .
```

This creates a local development candidate only. A distributable release still
requires registry digests, generated SBOMs, signatures/attestations, and a
verified release manifest.

The identity command refuses changed or untracked release inputs. The root
Docker ignore file also excludes the local journal, dependencies, generated
results, transcripts, runtime state, and archived applications so those bytes
cannot leak into the image or make otherwise identical builds diverge.

Candidate assembly and qualified Cosign verification are documented in
[`RELEASE.md`](RELEASE.md). Candidate integrity is not a release signature, and
a qualified verification requires a trusted public key supplied from outside
the downloaded bundle.

## Run on the dedicated runner

1. Create `/var/lib/stack-bench/{work,results,secrets,controller-home}` with
   access limited to the appliance operator. `controller-home` holds only
   controller CLI state and per-run agent transcripts; it is outside the
   read-only image and persists for artifact/audit collection.
2. Write the provider API key as the only line in
   `/var/lib/stack-bench/secrets/anthropic_api_key`; set mode `0600`.
3. Copy `secrets.env.example` to an operator-owned file and replace the two
   example image values with exact `@sha256:` references from the release
   manifest.
4. Pull and verify every manifest image before starting any service.
5. Render the Compose file and run the exact requested preflight.

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml config --quiet

docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  preflight --backend spacetime,postgres,mongodb \
  --track ecommerce --levels 1-2 --run-index 0 --smoke
```

After preflight is green, run only the scope the operator decided to test:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  run --backend postgres --track ecommerce --levels 1-2 --run-index 0
```

The report records the exact levels, packs, checks, stack adapter, agent
adapter, model, image content identity, and preflight evidence. It does not
infer a special label from a partial selection; it states what was run.

## Cleanup and recovery boundary

Results remain under `/var/lib/stack-bench/results` after the controller exits.
Do not delete that directory until its artifact manifest has been verified and
copied off the runner. The dependency and database volumes are release-owned,
but interruption-safe cleanup and quarantine instructions are SB-403 work and
are not yet a production release guarantee.
