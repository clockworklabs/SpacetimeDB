# LLM benchmark validation

The normal suite contains 95 tasks. The 12 `advanced` tasks are included by default,
in all three languages and in both context modes. They are also included in the
normal website task catalog. `--categories advanced` is only a development filter.

## New behavioral tasks

| ID | Contract checked |
| --- | --- |
| 083 | Two-product reservation, rollback, conflicting replay, concurrent last-unit claims |
| 084 | Tenant-filtered pagination, sparse matches, timestamp ties, insertions between pages |
| 085 | Batched backfill with interleaved writes, tombstones, retries and ID reuse |
| 086 | Aggregate updates, category moves, zero totals and repeated deletion |
| 087 | Webhook validation, account isolation, stale events, conflicting and concurrent retries |
| 088 | Server-clock leases, fencing tokens, expired and stale completion |
| 089 | Real scheduled expiry, renewal generations and durable cancellation |
| 090 | Per-identity transactional quota, rejected-request rollback and concurrent requests |
| 091 | Real scheduled queue, durable business failure, independent completion and duplicate requests |
| 092 | Procedure cache, exact composite key, TTL, upstream failure and zero-TTL bypass |
| 093 | Private source tables, owner checks, live permission revocation and reconnect |
| 094 | Real multi-connection presence, partial disconnect and reconnect |

These are SpacetimeDB contracts, not copies of Convex APIs. Pagination checks
correctness, not index scan cost. Backfill checks an online data migration
algorithm, not deployment schema conversion. Quota is lifetime-based, not a
timed rate limiter. Presence checks module behavior; it does not grade generated
client SDK cache code. Cache calls use a local HTTP fixture, never a paid service.

The existing transfer (055), aggregate (065), projection (068), presence (070),
and webhook (078) graders also have stronger checks. Task 065 now requires public
`set_sale` and `remove_sale` reducers, so a hard-coded `exercise` result cannot pass.
Scores from this suite are not directly comparable with the older 83-task suite.

## Check reference implementations without model calls or uploads

First build the local CLI and server, with loopback HTTP enabled for the cache
fixture. CI already does this through `cargo ci smoketests prepare`.

```text
cargo build --release -p spacetimedb-cli -p spacetimedb-standalone --features spacetimedb-standalone/allow_loopback_http_for_tests
dotnet pack -c Release crates/bindings-csharp/BSATN.Runtime
dotnet pack -c Release crates/bindings-csharp/Runtime
pnpm --dir crates/bindings-typescript build
```

Run this command once per language (`rust`, `csharp`, `typescript`):

```text
cargo run -p xtask-llm-benchmark --bin llm_benchmark --locked -- run --lang rust --goldens-only --dry-run --skip-task-catalog-upload --tasks t_055,t_065,t_068,t_070,t_078,t_083,t_084,t_085,t_086,t_087,t_088,t_089,t_090,t_091,t_092,t_093,t_094
```

The command publishes disposable local databases and runs the real graders. It
makes no model requests and uploads no scores or catalog. Omitting `--tasks`
validates all 95 reference tasks. Keep the weekly action disabled until these
checks pass and the branch is reviewed.

The scenario grader checks explicit expected rows, including intermediate state,
instead of accepting agreement between two copies of the reference answer.
Transport errors do not count as expected reducer failures. Presence observations
exclude their own temporary HTTP connection; HTTP SQL invokes lifecycle hooks.
Scheduled-callback checks accept native private-function rejection as well as an
explicit scheduler-identity error, then verify actual scheduled effects.
All network calls have timeouts, and local fixture/socket resources are closed on
failure as well as success.
