# LLM benchmark validation

The catalog contains 101 tasks. Default selection runs every task that has a prompt
for the requested language: 99 Rust, 100 C#, and 100 TypeScript tasks. All supported
tasks are included in both context modes and the normal website task catalog.
`--categories advanced` is only a development filter, not an opt-in gate.

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
| 095 | Parameterized per-user views, separate argument sets, live moves and access isolation |
| 096 | Server-clock token bucket, refill, request replay and concurrent admission |
| 097 | Seat allocation, FIFO waitlist, cancellation, rejoin and concurrent promotion |
| 098 | Nested independent C# submodules and cross-instance transaction rollback (.NET 10 only) |
| 099 | Two-database outbox delivery, sender rollback, receiver failure and result callbacks (Rust/TypeScript) |
| 100 | Actual SDK client cache recovery, lost call results and no unsafe retry (C#/TypeScript) |

These are SpacetimeDB contracts, not copies of Convex APIs. Pagination checks
correctness, not index scan cost. Backfill checks an online data migration
algorithm, not deployment schema conversion. Quota is lifetime-based, not a
timed rate limiter. Presence checks module behavior; it does not grade generated
client SDK cache code. Cache calls use a local HTTP fixture, never a paid service.

The existing transfer (055), aggregate (065), projection (068), presence (070),
and webhook (078) graders also have stronger checks. Task 065 now requires public
`set_sale` and `remove_sale` reducers, so a hard-coded `exercise` result cannot pass.
Event table task 024 checks live v2 events, repeated identical events and no history
for late subscribers. Scheduled cleanup task 058 checks each transaction's delete
count and group isolation, not only eventual emptiness. This does not measure scan cost.
Scores are not directly comparable with the older 83- or 95-task suites.

Task 100 asks for client code. Its server is fixed, and its supplied bindings use
the local SDK. A local TCP proxy lets a non-idempotent reducer commit, hides its
reply, then drops the connection. The grader checks an unknown outcome, retained
cache, subscription replay, stable identity, no duplicate callbacks and no retry.
It does not replace these checks with a raw WebSocket simulation of an SDK.
Generated clients run in read-only, non-root Docker containers with CPU, memory,
process and time limits. Client output is limited to 1 MiB. Only compiled client files are mounted. The container
receives no runner secrets, checkout or Docker socket. It shares only a network-disabled
relay's loopback interface. The relay accepts only WebSocket upgrades to the current
task's database, then forwards that connection over Docker stdio to the local test
server. The SDK's bodyless token-exchange endpoint is also allowed. Database HTTP
calls and other databases are blocked. No host port is opened, and the client cannot reach host services
or the Internet. Both containers have forced-stop deadlines if the scorer is killed.

## Check reference implementations without model calls or uploads

First build the local CLI and server, with loopback HTTP enabled for the cache
fixture. CI already does this through `cargo ci smoketests prepare`.

```text
cargo build --release -p spacetimedb-cli -p spacetimedb-standalone --features spacetimedb-standalone/allow_loopback_http_for_tests
dotnet pack -c Release crates/bindings-csharp/BSATN.Runtime
dotnet pack -c Release crates/bindings-csharp/Runtime
pnpm --dir crates/bindings-typescript build
docker pull node:22-bookworm-slim
docker pull mcr.microsoft.com/dotnet/runtime:10.0
node --test tools/xtask-llm-benchmark/src/eval/client_transport.test.mjs
```

Node.js 22 or newer is required for both TypeScript and C# client checks. C# uses
the repository's .NET 10 SDK and local client SDK project. A Linux Docker daemon
is required for client checks, including local reference validation. Task-local
`bin`, `obj`, and `node_modules` directories are removed after grading. The shared
SDK build output remains in its normal repository cache.

Run this command once per language (`rust`, `csharp`, `typescript`):

```text
cargo run -p xtask-llm-benchmark --bin llm_benchmark --locked -- run --lang rust --goldens-only --dry-run --skip-task-catalog-upload --tasks t_055,t_065,t_068,t_070,t_078,t_083,t_084,t_085,t_086,t_087,t_088,t_089,t_090,t_091,t_092,t_093,t_094
```

The command publishes disposable local databases and runs the real graders. It
makes no model requests and uploads no scores or catalog. Omitting `--tasks`
validates all supported reference tasks for that language. To check this addition,
use `--tasks t_024,t_058,t_095,t_096,t_097,t_098,t_099,t_100`; unsupported language
pairs are skipped. Keep the weekly action disabled until these
checks pass and the branch is reviewed.

The scenario grader checks explicit expected rows, including intermediate state,
instead of accepting agreement between two copies of the reference answer.
Transport errors do not count as expected reducer failures. Presence observations
exclude their own temporary HTTP connection; HTTP SQL invokes lifecycle hooks.
Scheduled-callback checks accept native private-function rejection as well as an
explicit scheduler-identity error, then verify actual scheduled effects.
All network calls have timeouts, and local fixture/socket resources are closed on
failure as well as success.

## Supplied bindings

The small interface projects in tasks 099 and 100 are inputs to the CLI generator.
They are not model answers. Regenerate 099 with `spacetime generate --bindings module`
for Rust and TypeScript. Regenerate 100 with client bindings for C# and TypeScript
(use `--import-extension js` for TypeScript). Keep their generated output in the
task's `fixtures/<language>/src/receiver` or `client/<language>/module_bindings`
directory. Do not hand-edit generated bindings.
