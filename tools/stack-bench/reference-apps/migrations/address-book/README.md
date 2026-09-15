# Address-book migration reference delta

Draft. This delta applies only to disposable copies of the recorded ecommerce
references. The starting references stay unchanged. Their cumulative source is
used for the selected migration slice; it is not an isolated L3 application.

The delta checks source identity and every patch anchor before writing files.
PostgreSQL and MongoDB import profiles during startup. SpacetimeDB startup
publishes the module without deleting data; reducers import on first access.
The saved source owns deployment. Each implementation keeps an import marker
so deleting the last address does not import it again.

## Run the reference diagnostic

Inside the existing Linux controller environment, from the package root:

```sh
M8_OUTPUT_ROOT=/path/to/private/evidence M8_CHECKPOINT_RUN=1 \
  node reference-apps/migrations/address-book/m8-migration.mjs postgres 83
```

Select `postgres`, `mongodb` or `spacetime`. The second argument is the runtime
run index. Use the normal Docker socket, shared state, release dependencies,
runtime image and resource-lock environment. No coding model is called.

Without `M8_CHECKPOINT_RUN=1`, an optional third argument selects a defect:
`no-import`, `wrong-owner`, `changed-history`, `stale-profile`, `resurrect`, or
`cross-account`. A defect run must fail for its intended observation; a process
failure is not proof that the observer detected the defect.

The diagnostic deploys and populates the starting app through supported actions.
It waits for scheduled delivery before recording stable business data. Fresh
reads, writes, application restarts and native stored records test import,
ownership, defaults, deletion, old-profile behavior and a new purchase.
SpacetimeDB also restarts its database process. These are normal restarts,
not crash or power-loss tests.

## Code and data restoration

The checkpoint trial uses the shared agent and repair-decision functions. It
preserves the first submission and rejected sources, detects real unauthorized
writes, and restores accepted code with its matching data. It also tests:

- Wrong workspace, mismatched source and empty-database controls.
- A data change with no source change.
- Repeated restoration and a new purchase after restoration.
- Saved candidate code replayed against the original populated database.
- A no-import implementation that passes on already migrated data, but must
  fail when replayed from the original population.

Cold checkpoints stop application writers and the native database process.
They keep the leased container and network. Source, archive, image, container,
workspace and lease identities must match before restoration. Failure stops
the diagnostic.

Keep checkpoints private. They contain account data and credentials. They are
valid only for the same live lease and image, not for export or another run.
The clock is not restored. Active timed operations are outside this pilot.

## Limits

These scripts qualify reference behavior and shared runtime boundaries. The
normal compiled benchmark task, grading and repair lifecycle remain incomplete.
No scored recipe or qualification is promoted by running this diagnostic.
The diagnostic's stored-state readers assume known reference storage. The new
grader address-book reader uses the declared authenticated interface with opaque
string IDs. Original business-state comparison still needs its migration-safe
reader and matching qualification.

The direct diagnostic currently releases its lease on normal completion or a
caught error. Direct SIGINT/SIGTERM can skip that cleanup. Final execution must
use the normal runner's signal teardown; cancellation is not yet qualified.

Keep failed, incomplete and successful evidence together. Receipts include
source hashes, image identity, observations, session costs and cleanup results.
The private local audits retain development failures and earlier protocol
versions. Passing an earlier version does not qualify a changed protocol.
