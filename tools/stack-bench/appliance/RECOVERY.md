# Interruption and recovery

Stack Bench never guesses that a container, listener, lock, database, or data
directory is safe to delete. Normal teardown authenticates the run's private
lease, compares exact container IDs and listener PIDs, and releases only locks
whose owner record still matches that lease.

Every appliance run keeps two different records:

- `results/.../recovery.json` is public, contains no ownership token, and says
  whether cleanup is `clean`, intentionally `retained`, or `quarantined`;
- `controller-home/supervisor/<run-id>.json` is private recovery authority. It
  contains the lease token and must remain readable only by the appliance
  operator. Normal cleanup deletes it. Refused cleanup deliberately preserves
  it.

## If a run is interrupted

1. Preserve the result directory and private supervisor-state file.
2. Read `recovery.json`. Do not publish an attempt whose status is
   `quarantined`.
3. Do not start another run using any lock key listed in that artifact.
4. Retry authenticated cleanup from the controller:

```sh
docker compose --env-file /var/lib/stack-bench/operator.env \
  -f appliance/docker-compose.yaml run --rm controller \
  recover /var/lib/stack-bench/controller-home/supervisor/<run-id>.json
```

On success the command changes `recovery.json` to `clean`, releases the exact
owned resources, and removes the private supervisor state. It is idempotent
when public lease evidence already proves that an earlier cleanup completed.

## If recovery refuses

Refusal is the safety behavior. It means a live resource does not match the
lease or its identity could not be proven. The command leaves the private state,
lease, lock records, and public quarantine artifact intact.

Compare the live container ID and listener PIDs with `recovery.json` and the
private lease before manual action. Never delete a same-name container, kill a
port's current listener, remove another lock, or recursively clear the shared
state root merely because its name resembles Stack Bench. Escalate with the
complete result directory and private state stored separately from public
artifacts.

## Intentional retention

`--retain-backend` is inspection mode, not successful cleanup. It writes
`status: "retained"` and preserves private recovery authority. No other run may
reuse the listed locks until the recovery command completes.
