# Stack Bench appliance

The appliance is the supported v1 deployment for paid and subscription-backed
Stack Bench campaigns. Run it on a dedicated, disposable Linux/amd64 machine
with a local Docker Engine and the persistent path `/var/lib/stack-bench`.

The controller has root-equivalent access through the Docker socket. Do not run
it beside unrelated workloads or credentials. Copy verified results off the
runner before destroying it.

See [appliance design](DESIGN.md) for the security model and
[release guidance](RELEASE.md) for bundle assembly and signature verification.

## Package contents

- `Controller.Dockerfile` builds the controller and grader.
- `docker-compose.yaml` starts the controller, dependency initializer,
  PostgreSQL, MongoDB, and optional dashboard.
- `operator.env.example` lists operator configuration without secret values.
- the dependency initializer copies the release SDK, CLI, and runtime into a
  checksummed read-only volume.

The coding container receives one app workspace, its transcript directory, and
only the dependencies declared by its stack adapter. It cannot read the
controller, grader, scenarios, results, or provider credential.

## Prepare the runner

1. Create these operator-only directories:

   ```text
   /var/lib/stack-bench/work
   /var/lib/stack-bench/results
   /var/lib/stack-bench/secrets
   /var/lib/stack-bench/controller-home
   ```

2. Configure one provider credential mode:

   - For subscription billing, run `claude setup-token` and write only the
     returned token to `secrets/claude_subscription_token`.
   - For API billing, write the provider key to a file under `secrets/`.

   Set secret-file permissions to `0600`. Do not configure both modes.

3. Copy `operator.env.example` to `/var/lib/stack-bench/operator.env`. Replace
   example image values with the exact digest references from the release
   manifest.

4. Copy the release manifest to the configured path. Pull and verify every
   manifest image before starting services.

5. If you use the dashboard, write at least 32 random characters to
   `secrets/dashboard_control_secret` and set its mode to `0600`.

## Validate the appliance

Run commands from `tools/stack-bench` on the runner.

Check the Compose configuration:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml config --quiet
```

Run preflight for the exact planned scope:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  preflight --backend spacetime,postgres,mongodb \
  --track ecommerce --levels 1-2 --run-index 0 --smoke
```

Preflight verifies the runner, credentials, images, dependencies, ports,
network paths, storage, stack access, and required qualification evidence. The
smoke check uses the real build image and does not call a model.

## Inspect a campaign

The campaign file is the run authority. Store it below
`/var/lib/stack-bench/results/plans`.

Compile and inspect it without model work:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  campaign show /var/lib/stack-bench/results/plans/campaign.json
```

A frozen campaign binds the model, stacks, work, checks, budgets, repetitions,
parallelism, pricing, controller image, and build image. The runner must match
those identities.

Frozen means immutable. It does not mean the grading checks are qualified.
The plan, dashboard, and report show qualification status. Publish scores as
verified comparison data only after every selected level is qualified.

`campaign.example.json` is a zero-cost deterministic example. Copy it outside
the image before changing it.

## Run a campaign

Prepare durable state without starting an attempt:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  campaign prepare /var/lib/stack-bench/results/plans/campaign.json \
  --out /var/lib/stack-bench/results/campaigns/campaign-001
```

Start the exact frozen plan:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  campaign run /var/lib/stack-bench/results/plans/campaign.json \
  --out /var/lib/stack-bench/results/campaigns/campaign-001
```

The campaign controls attempt counts and concurrency. `repetitions` sets the
default attempt count per stack. A stack can override it. `parallelism` limits
simultaneous attempts. Each live attempt receives isolated ports, database
names, locks, workspaces, and evidence paths.

The remaining `campaign` snippets are controller subcommands. Run them after
the same Docker Compose `run --rm controller` prefix used above.

Use durable state for normal control:

```sh
campaign status <campaign-directory>
campaign inspect <campaign-directory>
campaign report <campaign-directory>
```

- `status` is the compact normal view.
- `inspect` adds score, cost, duration, cleanup, evidence, and feature progress.
- `report` rebuilds `report/report.json` and `report/report.html` from retained
  evidence.

Do not infer state from logs. Use logs only to diagnose a reported phase or
failure. The controller never retries, extends, or grants paid work
automatically.

## Resume and repair

If the controller stopped while an attempt remained live, reconcile ownership
before any resume:

```sh
campaign reconcile <campaign.json> --out <campaign-directory>
```

Reconciliation changes state only when private supervisor evidence proves that
the exact owned resources are clean.

Dependency campaigns can grant more strikes to selected exhausted features:

```sh
campaign grant-strikes <campaign-directory> \
  --attempt <attempt-id> --grant-id <unique-id> --level <N> \
  --feature <feature-id> --strikes <N>
```

The grant creates a linked continuation. It does not rewrite the completed
execution. Use `campaign resume <campaign.json> --out <campaign-directory>` to
run scheduled dependency work.

## Model-free trials and qualification

`campaign trial` accepts only registered non-billable adapters and zero pricing.
It validates orchestration but does not produce comparative model data.

Check qualification requirements without starting work:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  qualification status --track ecommerce --level <N>
```

Run only evidence required by that exact status. Do not repeat reference,
mutation, or null work when its bound inputs have not changed. See the
[reference app guide](../reference-apps/README.md) and
[grader guide](../grader/README.md) for qualification rules.

## Dashboard

Start the optional dashboard:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml --profile dashboard up -d dashboard
```

Open `http://127.0.0.1:7331`. The dashboard reads and controls the same campaign
state as the CLI. See [dashboard/README.md](../dashboard/README.md).

## Results and cleanup

Results remain under `/var/lib/stack-bench/results` after the controller exits.
Verify and copy the complete campaign package before deleting the runner.

A run removes only resources whose private ownership evidence still matches.
If cleanup cannot be proved, it preserves the evidence and quarantines the run.
Follow [RECOVERY.md](RECOVERY.md). Do not delete same-name resources or clear the
shared state root by guesswork.
