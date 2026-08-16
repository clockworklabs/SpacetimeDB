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
- `operator.env.example` documents the three operator-supplied values. It never
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
   controller CLI state and the CLI's live transcript cache; it is outside the
   read-only image. Completed runs archive transcripts and the generated
   SpacetimeDB friction log under `results/` for durable artifact collection.
2. For subscription billing, run `claude setup-token` once for the dedicated
   runner, write only the returned token to
   `/var/lib/stack-bench/secrets/claude_subscription_token`, and set the file to
   mode `0600`. Select `subscription-token` in the operator environment. The
   older rotating interactive-login file remains available as the explicit
   `credentials` mode at
   `/var/lib/stack-bench/controller-home/.claude/.credentials.json`. For API
   billing, write the provider API key as the only line in a mode-`0600` file
   below `/var/lib/stack-bench/secrets` and select `api-key`. Never configure
   more than one mode for a run.
3. Copy `operator.env.example` to `/var/lib/stack-bench/operator.env`, select the
   intended credential mode, replace the two
   example image values with exact `@sha256:` references from the release
   manifest, and copy that manifest to the configured path below
   `/var/lib/stack-bench`.
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

Compile and inspect a campaign without starting model work:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  campaign show /var/lib/stack-bench/plans/campaign.json
```

The campaign file is caller-owned under `/var/lib/stack-bench`. A draft can be
inspected while definitions are still candidates. A frozen campaign requires
qualified/promoted definitions plus exact controller and build image identities;
the compiler refuses to fill those in from ambient state. An internal campaign
may leave `releaseManifestSha256` null so distribution packaging does not block
measurement. When a distributed release manifest is named, execution hashes and
validates it and checks its controller/build-sandbox image references. In either
case, the running Compose controller must match the campaign. This runtime binding
does not replace separate release-bundle signature/integrity verification.
`campaign.example.json` is a zero-cost deterministic draft showing the complete
shape; copy it outside the image and replace its study inputs before use.

Prepare durable state without launching an attempt, then run that exact frozen
plan (or resume its remaining attempts) from the same persistent directory:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  campaign prepare /var/lib/stack-bench/plans/campaign.json \
  --out /var/lib/stack-bench/results/campaign-001

docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  campaign run /var/lib/stack-bench/plans/campaign.json \
  --out /var/lib/stack-bench/results/campaign-001
```

`campaign status /var/lib/stack-bench/results/campaign-001` reads the durable
state. Two controllers cannot own the directory at once. Failed harness
attempts remain visible and retries append new execution records. If a
controller ends while an attempt is still marked running, automatic resume
refuses. Run `campaign reconcile <campaign.json> --out <campaign-directory>`;
it advances the record only if the private supervisor evidence proves exact-
owned cleanup. It never invents a result or silently starts a duplicate.

After any completed or stopped campaign, regenerate the report only from its
stored evidence:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  campaign report /var/lib/stack-bench/results/campaign-001
```

This writes `report/report.json` and self-contained `report/report.html`. The
report states the exact scope and campaign status, keeps invalid executions and
retries visible, links raw run and admission artifacts, includes exact recipe,
fixture, calibration, runtime-image, release, and pricing identities, applies
the declared dispersion, and does not impute missing metrics. When a condition
selects unmentioned first-build probes, the report gives them a separate
diagnostic section, denominator, coverage value, and raw-evidence link. Probe
observations never alter the requested score or correction metrics. Deleting
only the `report` directory and running the command again produces the same
report identity and bytes.

Qualification is also an explicit appliance operation. Select one validated
track/level and backend; reference and mutation evidence are separate retained
artifacts, while the null gate is stack-independent:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  preflight --agent-adapter reference-fixture \
  --backend mongodb,postgres,spacetime --track ecommerce --levels 2 \
  --run-index 0 --smoke

docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  qualification status --track ecommerce --level 2
```

The `reference-fixture` adapter makes this qualification preflight model-free;
it does not call a provider or spend model tokens. Paid campaign preflight must
instead select the campaign's real agent adapter and have the credential mode
used by that campaign ready.

Each scoped qualification artifact binds the executable calibration identity.
Evidence from another recipe, fixture, mutation set, control policy, or declared
repetition plan cannot be substituted during promotion.
Reference qualification also retains each complete underlying benchmark run in
a sibling `<artifact-name>.runs/` directory. Keep that directory with the JSON;
the run paths recorded in the artifact are relative to the artifact itself.
New reference and null-control artifacts record the controller mode, operating
system, CPU architecture, Docker Engine version, Docker-reported kernel,
Docker architecture, CPU allocation, and memory allocation. A calibration may
bind a supported class of runner; ecommerce L2 requires the Linux/amd64
appliance for reference, mutation, null, and budget evidence. Every artifact in
one qualification or budget-measurement set must also report the same complete
runner snapshot, so timings from materially different environments cannot be
silently combined.
Local-controller runs remain useful diagnostics but cannot promote that recipe.
Artifacts created before runner identity or the complete runner observation was
recorded remain readable, but are not accepted where the selected calibration
requires that evidence.

Before the first qualification of a recipe whose packs still have unmeasured
runtime budgets, run each `budgetPreparation.commands` entry printed by
`qualification status` through the same Compose prefix shown above. Those
commands collect pristine references for every supported stack and then run
`pack-budget recommend`. The recommendation command verifies the exact recipe,
calibration, fixture, engine, stack coverage, repetitions, controller
environment, retained raw runs, and component arithmetic. Its policy takes the
largest observed pack runtime, doubles it, and rounds upward to the next second.
It writes a review artifact and never edits pack definitions.

Review the recommendation, apply the accepted bounds to the pack definitions,
commit them, and build a new exact controller image. The budget-measurement
artifacts are inputs to that source change; they are not the final qualification
evidence because the executable identity changes when the bounds are added.
On the new image, require a green preflight and run every command in the
`commands` array through the Compose prefix. Only those post-budget reference,
mutation, and null artifacts can be bound to promotion.

Before launching the official repetitions, inspect the exact go/no-go record:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  qualification status --track ecommerce --level 2
```

It is read-only. The JSON separates launch blockers, required evidence and
commands, promotion blockers, and the governance states promotion would change.
It never supplies a missing runtime budget or treats an absent artifact as a
pass.

To run one already-decided attempt directly:

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
This includes archived model transcripts and the SpacetimeDB friction log; they
must not depend on the CLI's 30-day live-cache retention.
Do not delete that directory until its artifact manifest has been verified and
copied off the runner. Every run writes a public recovery status and keeps
private authenticated recovery authority until exact-owned cleanup succeeds.
Follow [`RECOVERY.md`](RECOVERY.md) for interruption, quarantine, safe retry, and
intentional retention. Dependency and database volume destruction remains an
operator action after verified result export; no run recursively deletes the
shared state root.
