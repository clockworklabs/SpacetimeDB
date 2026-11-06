---
name: recover-run
description: Clean up after an interrupted StackBench run or campaign without losing evidence
metadata:
  author: clockworklabs
  role: operator
---

# Recover an interrupted run

StackBench never deletes a container, lock, or database on a guess. Cleanup
proves ownership first, from a private lease file that holds the run's token.
Recovery reruns that proof. It does not restore data or restart the agent.

Every command runs from `tools/stack-bench`. `controller` means:

```sh
docker compose --env-file operator.env -f appliance/docker-compose.yaml run --rm controller
```

`<state-root>` is the `STACK_BENCH_STATE_ROOT` value in `operator.env`.

## Rules

- Recover before removing anything. Removing `sb-*` containers first quarantines
  the run and leaves its locks held.
- Never delete a same-name container, kill a port's listener, or clear the
  state root because the name looks like StackBench's.
- Never publish an attempt whose `recovery.json` says `quarantined`.

## An interrupted campaign

```sh
controller campaign reconcile plans/<plan>.json --out campaigns/<campaign>
```

Use the campaign's original plan and output directory. Reconciliation checks
each attempt's lease, releases what it can prove, and updates campaign state.
Then `campaign resume` continues pending work.

## A single run

1. Read `results/.../recovery.json`. It says `clean`, `retained`, or
   `quarantined`.
2. Run authenticated cleanup from the supervisor record:

   ```sh
   controller recover <state-root>/controller-home/supervisor/<run-id>.json
   ```

   On success it marks the run `clean`, releases its containers, network, and
   locks, and deletes the supervisor record. Rerunning it is safe.

3. If there is no supervisor record (the parent process died first), recover
   from the lease instead, with an output directory outside the runtime folder:

   ```sh
   controller recover-lease <state-root>/controller-home/runtime/<run-id>/backend-lease.json --out <state-root>/results/recovery/<run-id>
   ```

## If recovery refuses

Refusal means a live resource does not match the lease. It leaves everything in
place. Compare the container and network IDs in `recovery.json` and the lease
with `docker ps -a` before doing anything by hand.

The common case: Docker or the host restarted and stopped the build container.
Recovery then says `workspace handback requires running build container <id>`.
Confirm the ID matches the lease, start that exact container with
`docker start <id>`, and run `recover` again.

If the build container was removed, automatic recovery cannot prove the
handback. Keep the result directory and the private state, and repair workspace
ownership by hand.

## Retained runs

A run started with `--retain-backend` keeps its containers for inspection and
reports `retained`. Its locks stay held until you run `recover` on it.
