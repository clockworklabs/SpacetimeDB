# Address-book migration reference delta

Draft. This source delta is applied only to disposable copies of the recorded
ecommerce starting references. The starting references stay unchanged.

`src/references/address-book-migration.ts` checks the starting source identity
and patch anchors. It adds the native implementation without deploying or
resetting the database. Startup or first use performs migration against the
retained database. Each live receipt must include both source identities.

Implemented: PostgreSQL transactions, MongoDB transactions with an embedded
address book, and SpacetimeDB reducers with private tables and an owner view.
PostgreSQL and MongoDB import on startup. SpacetimeDB imports on first access.
Each records an import marker that survives deletion of the last address.

## Live reference diagnostic

Run inside the existing controller environment, from the package root, after
building its TypeScript code:

```sh
M8_OUTPUT_ROOT=/path/to/persistent/evidence \
  node reference-apps/migrations/address-book/m8-migration.mjs postgres 83
```

The backend is `postgres`, `mongodb` or `spacetime`. The second argument is the
existing runtime's run index. An optional third argument selects a deliberate
defect: `no-import`, `wrong-owner`, `changed-history`, `stale-profile`,
`resurrect` or `cross-account`. A detected defect returns a nonzero exit;
inspect its error and phase, since an execution failure is not proof that the
observer detected the intended defect.

Use the controller's normal Docker socket, shared state, release dependency
volume, runtime image and resource-lock environment. The diagnostic calls the
existing reference deployment, grader, lease, restart and teardown operations.
It does not call a model. It refuses a source other than the recorded reference.

The diagnostic creates populated starting data through the app. It waits for
the scheduled delivery to finish before saving a stable baseline. It then
applies the source delta without resetting data. Fresh authenticated reads and
writes are checked against stored records. The final purchase uses the existing
checkout reconciliation. PostgreSQL and MongoDB repeat application startup
against the retained database. SpacetimeDB also restarts its database process.
These are normal restarts, not crash or power-loss tests.

The three `.mjs` files are reference-only diagnostics. Their native operations
and stored-state readers assume the exact source mapping; they are not a
generic protocol for model-generated apps. Receipts retain the starting and
migrated source hashes, controller image, observations, command logs and cleanup
result. Keep failed attempts with successful ones.

No registered recipe, scored check or qualification status changes here.
The draft specification still needs per-check controls and integration before
scored promotion. Fault recovery, concurrent migration and full database
coverage are not established by this diagnostic.

## Populated repair checkpoints

Set `M8_CHECKPOINT_RUN=1` on a correct reference diagnostic to test the shared
repair restore path with populated data. It captures both the original app and
the migrated app. Each checkpoint pairs source with a cold native database
archive. Application writers and the database process stop during capture;
the leased container and its network remain in place.

The diagnostic checks that an empty database reset loses the populated state,
then recovers it. It rejects a mismatched source, changes code and data twice,
restores the accepted pair twice, and restores the original pair twice. A new
purchase after restoration must have the expected stored order and stock effects.
A detached application process must also stop before capture.

These checkpoints are private and valid only for the same live lease, container
and image. They include credentials. Do not publish or copy them to another run.
Archive or source changes cause validation to fail before restoration starts.
A failed capture or restore stops the diagnostic; it is not a passed repair.
The clock is not restored, so this pilot waits for scheduled delivery to finish
before capture. It does not cover active timed operations.

This qualifies the restore boundary, not a complete migration agent episode.
The normal benchmark command still uses its existing fresh-database policy;
populated task setup, grading and repair selection still need integration.

On 15 September, the final checkpoint protocol passed on all three stacks with
image `sha256:763510743605256bd86cabca627b316dbb414462360557a550ae0320a389761e`.
Each passed the empty-reset and mismatched-source controls, two accepted-pair
restores, two original-pair restores, writer cleanup and a new stored purchase.
Type checking, 52 focused tests and 23 Linux tests passed without skips in those
final test sets. The Linux set includes the existing schema-rollback integration.
All 11 development attempts remain recorded: seven passed and four failed.
All owned containers were released. Earlier failures exposed MongoDB shutdown
permissions, an order-sensitive comparator, an unsynchronized cart probe and a
profile-save timeout. The migration copy now preserves unsaved profile edits
across refresh; the old timeout's exact request sequence was not captured.
These trials made no model calls and changed no benchmark scores.

## Local validation, 14 September 2026

Code `8dd180fa6` passed one positive migration and detected all six selected
defects on each stack: 21 planned, started and measured cases. All 21 stored
source hashes matched their records, and all owned run containers were released.
Package type checking and 48 focused tests passed. The diagnostic image was
`sha256:41f12f5d9552e3b8633521a482f65b9c9efe6ebc19cc942ff05379957947ef1c`.

The earlier 29 development attempts remain in the local audit, including nine
unqualified attempts. Their issues included a container argument error, a pinned
SDK callback mismatch, an invalid negative-control build, scheduled delivery
changing the baseline, and three browser setup timeouts. The timeout cause is
not proven by the later passes. These results qualify this reference stage;
they do not promote a scored migration task or establish model performance.
