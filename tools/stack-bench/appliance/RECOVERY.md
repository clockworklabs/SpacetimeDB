# Interruption and recovery

Recovery here means authenticated cleanup and state reconciliation. It does not
restore a database snapshot or restart an interrupted agent session. A planned
[depth pause](../README.md#pause-before-a-later-depth) retains live processes and
requires the original controller to stay running. After controller loss,
`continue-depth` refuses release and keeps the pause evidence unchanged.

Stack Bench never guesses that a container, listener, lock, database, or data
directory is safe to delete. Normal teardown authenticates the run's private
lease, compares exact owned container and network IDs, and releases only locks
whose owner record still matches that lease.

Paths below are inside the Docker state volume. Replace `<state-root>` with
the exact `STACK_BENCH_STATE_ROOT` value written by setup in `operator.env`.
Run Compose commands from `tools/stack-bench`. The host does not need that
Linux directory. Compose mounts the state volume at the recorded path.

Every appliance run keeps two different records:

- `results/.../recovery.json` is public, contains no ownership token, and says
  whether cleanup is `clean`, intentionally `retained`, or `quarantined`;
- `<state-root>/controller-home/supervisor/<run-id>.json` is private recovery authority for a standalone run. It
  contains the lease token and must remain readable only by the appliance
  operator. Normal cleanup deletes it. Refused cleanup deliberately preserves
  it.

Campaign supervisor records are under that campaign's `.private` directory.
For an interrupted campaign, first use the campaign recovery path:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller \
  campaign reconcile plans/campaign.json --out campaigns/campaign-001
```

Use the campaign's original plan and output directory. Reconciliation checks
private child authority before it releases reservations or changes campaign
state. The direct commands below are for a specific retained supervisor or
lease path reported by the run.

## If a run is interrupted

1. Preserve the result directory and private supervisor-state file.
2. Read `recovery.json`. Do not publish an attempt whose status is
   `quarantined`.
3. Do not start another run using any lock key listed in that artifact.
4. Retry authenticated cleanup from the controller:

```sh
docker compose --env-file operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  recover <state-root>/controller-home/supervisor/<run-id>.json
```

On success the command changes `recovery.json` to `clean`, releases the exact
owned resources, and removes the private supervisor state. It is idempotent
when public lease evidence already proves that an earlier cleanup completed.

If the parent process ended before it retained a supervisor file, recover from
the private runtime lease instead. Supply a durable output directory outside
the private runtime directory:

```sh
docker compose --env-file operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  recover-lease <state-root>/controller-home/runtime/<run-id>/backend-lease.json \
  --out <state-root>/results/recovery/<run-id>
```

This path uses the same ownership token, container ID, network ID, and lock
checks. It refuses an output directory inside the runtime directory because a
successful recovery removes that directory.

## If recovery refuses

Refusal is the safety behavior. It means a live resource does not match the
lease or its identity could not be proven. The command leaves the private state,
lease, lock records, and public quarantine artifact intact.

Compare the live container and network IDs with `recovery.json` and the
private lease before manual action. Never delete a same-name container, kill a
port's current listener, remove another lock, or recursively clear the shared
state root merely because its name resembles Stack Bench. Escalate with the
complete result directory and private state stored separately from public
artifacts.

## Intentional retention

`--retain-backend` is inspection mode, not successful cleanup. It writes
`status: "retained"` and preserves private recovery authority. No other run may
reuse the listed locks until the recovery command completes.
