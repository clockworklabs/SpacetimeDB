# LLM benchmark validation

The catalog contains 103 tasks. Default selection runs every task that has a prompt
for the requested language: 101 Rust, 102 C#, and 103 TypeScript tasks. All supported
tasks are included in both context modes and the normal website task catalog.
`--categories advanced` is only a development filter, not an opt-in gate.

Provider errors or missing response content reject the whole model/language/mode
batch after retries. No partial scores from that batch are uploaded. Other models
still run, but the language job exits with failure if any batch fails. Response
errors include the generation ID and finish reason when supplied by the provider.
Generated code that fails compilation or grading remains a normal benchmark score.
A provider rejecting the content also scores: the task fails all of its checks, with
no retry. The batch still completes and uploads.

C# uses a fresh server per model batch. Each active task publishes its own reference
database and deletes it after scoring or a provider failure. This bounds live C#
references by task concurrency, rather than loading all 96 before model generation.
`--goldens-only` also validates and deletes each C# reference/candidate pair before
starting the next task. Rust and TypeScript keep their batched reference builds.
Cleanup errors block the batch, but completed scoring remains in the local record.
Compiler errors in SDK-generated files score as normal failures. Saved batch
records keep the evidence so suspected SDK defects can go to the core team.
Schema parity treats table and view names that differ only by case or underscores
as the same, because prompts do not fix their casing. It matches lifecycle reducers
by kind, not by name, because clients cannot call them. It accepts extra indexes and
HTTP routes but fails a missing one. It ignores view primary keys except on the task that asks for
one. Behaviour checks
query a table or view by the module's own spelling when it differs only by case or
underscores.

Each batch also saves an append-only JSONL record under
`LLM_BENCHMARK_REPORT_DIR/batches`, or `tools/xtask-llm-benchmark/runs/batches`
when that variable is not set. CI retains these files in its analysis artifacts,
including on failure. They are not website uploads.

The record contains the planned tasks, model identity, each finished task attempt,
generated code, scorer details, and provider refusal details. Outer task retries
remain in the record. The latest task attempt determines its status and the
summary. Inner provider-call retries are not separate artifact entries; their
status and retry counts remain in console logs. Raw refusal text, partial provider
content, and HTTP error bodies stay in artifacts, not console errors. Treat these
artifacts as source and provider-response data.

Provider and infrastructure errors have no code score. Summaries keep task counts
separate from scorer-check counts and always include coverage. For example, 99
passing tasks and one provider error means **99/99 scored tasks passed, with 99/100
tasks scored and one provider error**. It does not mean a complete 100% benchmark.
Zero scored tasks means no score, not 0%. An incomplete batch stays off the website.
A content rejection is a failed task, so 99 passing tasks and one rejection means
**99/100 tasks passed, with `refused_tasks` at 1**. A blank reply at the output limit
is retried first, then counts the same way.
Model mistakes have no fixed list of messages, so any candidate publish failure
scores normally unless it matches a short list of environment errors, such as a
full disk or a refused connection, or the server fails a health check afterward.
Migration setup failures also block the batch. A batch only completes if a known-good
module still publishes at the end, so a build environment that breaks mid-run cannot
upload misleading zeroes. A brief outage that matches none of these checks can score
one task as a failure; its error stays in the batch record.

Each finished attempt is saved before an error can stop the batch. If a task
reports an infrastructure error, new tasks stop, but started tasks finish and save
their results before the batch returns failure. If the process stops during a
write, only the last JSONL line can be incomplete; earlier lines
remain usable. Only a final `complete` event after the server health check marks
the batch complete for scoring. It does not confirm a website upload. A record
write failure blocks upload. This does not resume or upload partial runs.

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
| 098 | Nested independent submodules and cross-instance transaction rollback (C# on .NET 10 and TypeScript) |
| 099 | Two-database outbox delivery, sender rollback, receiver failure and result callbacks (Rust/TypeScript) |
| 100 | Actual SDK client cache recovery, lost call results and no unsafe retry (C#/TypeScript) |
| 101 | Typed environment declarations read from reducers and views, with hidden published values |

These are SpacetimeDB contracts, not copies of Convex APIs. Pagination checks
correctness, not index scan cost. Backfill checks an online data migration
algorithm, not deployment schema conversion. Quota is lifetime-based, not a
timed rate limiter. Presence checks module behavior; it does not grade generated
client SDK cache code. Cache calls use a local HTTP fixture, never a paid service.

The existing transfer (055), aggregate (065), projection (068), presence (070),
and webhook (078) graders also have stronger checks. Task 065 now requires public
`set_sale` and `remove_sale` reducers, so a hard-coded `exercise` result cannot pass.
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
use `--tasks t_095,t_096,t_097,t_098,t_099,t_100,t_101`; unsupported language
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
