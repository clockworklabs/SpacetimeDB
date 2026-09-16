# Stack Bench development

This guide covers local source development. Use the
[appliance guide](../appliance/README.md) for runner configuration, credentials,
preflight, campaigns, and paid model work.

## Requirements

- Node.js 22 or newer
- Docker Engine with Compose v2
- Chromium installed through the pinned Playwright dependency
- Linux for campaign and resource-lock tests; use a Docker development container
  when the host is not Linux

Install the locked dependencies and browser:

```bash
cd tools/stack-bench
npm ci
npm run bootstrap:browsers
```

Build the local coding image:

```bash
docker build --platform linux/amd64 -t stack-bench-build:local container
```

For real stack execution, use the [appliance build and setup](../appliance/README.md).
That build produces the CLI, server, and SDK from the branch. It needs no host
Rust build or ignored binaries. The appliance resolves local image tags to
immutable image IDs; published bundles use verified digest references.

## Source checks

Run the smallest check that covers the change:

| Change | Check |
|---|---|
| TypeScript | `npm run typecheck` and the focused compiled test |
| Unit tests | `npm test` |
| Dashboard read model, routes, and pages | `npm run test:dashboard` |
| Repository contracts | `npm run test:contracts` |
| Mutation definitions and anchors | `npm run test:mutation-definitions` |
| Browser, process, and Docker integration | `npm run test:integration` |
| Prompt composition | `npm run check:prompts` |
| Track scenarios | `npm run check:scenarios` |
| Packs and recipes | `npm run check:composition` |
| Calibration | `npm run check:calibration` |
| Dependency graph | `npm run graph` |

After a shared runtime, composition, grading, campaign, or release change is
stable, run the integrated source gate once:

```bash
npm run lint
npm run typecheck
npm run test:all
```

Use `npm test` while changing code. Run `npm run test:dashboard` when the
dashboard read model, routes, or pages change; it writes thirty campaigns of
fixture evidence and stays out of the unit tier. Run `npm run test:contracts`
when tracks, prompts, reference applications, repository policies, or campaign
definitions change. `npm run test:all` runs the unit, dashboard, and contract
tiers after one build. Docker and qualification checks remain separate.
Campaign and lock tests exercise native Linux `flock`. A Windows host cannot
run those tests directly. Portable compiler and definition tests still run
locally. For a clean branch, the existing controller Dockerfile's `source`
target contains the source, development dependencies, and compiled tests:

```sh
# From the repository root; this target does not build the Rust binaries.
docker build --platform linux/amd64 --target source -f tools/stack-bench/appliance/Controller.Dockerfile -t stack-bench-source-tests:local .
docker run --rm --init --network none stack-bench-source-tests:local npm run test:all
```

The release source build requires a clean normal Git clone. During development,
use a Linux container with the current edited checkout and locked dependencies.
Mutation-definition tests are model-free. Run them when reference source, grading
checks, or mutation manifests change. They do not run during ordinary unit work.

Documentation-only changes need link and formatting checks, not the harness.
Run Docker checks only when the changed code affects their boundary. Run
targeted mutations while developing checks and the complete mutation set only
for a release candidate. Integration files run sequentially because they can
own browsers, processes, ports, and Docker resources.

A passing check stays valid until one of its inputs changes. Do not rerun it for
reassurance. Add a test only when it protects a distinct invariant that an
existing test does not cover. Pending qualification marks campaign scores as
provisional; it blocks publishing verified comparisons, not campaign execution.

## Optional contention diagnostic

`tracks/ecommerce/scenarios/diagnostic-checkout-contention.json` is a separate,
zero-point diagnostic. It does not run in scored campaigns. On a reset,
disposable reference app copy with its authenticated lease environment, use the existing
grader entry point:

```sh
node dist/grader/grade.js --backend <stack> --app <reference-source> --url <app-url> --level 2 --spec tracks/ecommerce/scenarios/diagnostic-checkout-contention.json --out <diagnostic-result.json>
```

This standalone command deliberately omits `--track`. Both diagnostics include their named action mappings. Output is unbound to a recipe and has zero scored points; it cannot establish campaign completion. Backend reads still require the authenticated backend lease.

This starts no coding agent. It changes application data, so do not point it at a
campaign app or its retained database. Prepare each stack with the same runtime
and resources. The probe requires the declared stock data interface and two
sessions of each fresh test account. It sends 1, 4, 16, and 64 parallel checkout
requests, with three fresh-account cohorts per width. These are request counts,
not distinct client counts or sustained throughput.

The checkout diagnostic currently requires the verified reference schema. It does
not guess the schema of a generated app. Saved apps need a separate audited mapping.
Schema fingerprints are recorded with each stored-state observation. Unavailable
or malformed reads stay unmeasured, rather than becoming empty data or app failures.

Each cohort records stored state before adding the item, after preparing the cart,
and after checkout. It requires one order with the correct owner, line quantities,
prices and warehouse allocations, one booked payment, the exact stock change,
and an empty cart with no remaining reservations. Existing orders and payments
must remain intact. PostgreSQL stores payment fields on the order; MongoDB and
SpacetimeDB use separate payment records. The shared assertion accepts either
stock reservation during cart preparation or stock consumption at checkout.
Rejecting all requests cannot pass. Retained action evidence contains each
request's timing, response status, and transport error or timeout. Timings cover
client dispatch through response, not server overlap or commit latency. Inspect
these observations separately from correctness. This draft diagnostic still
needs matching reference and targeted-defect qualification.

Use `diagnostic-purchase-contention.json` with the same command to test competing
affordable purchases. It uses the declared `data-buy-input` interface and two
fresh customer accounts per cohort. Stock is reset to 128 in East and zero in
West before each cohort. Every request must be accepted, each account must show
its exact order count, and stored stock must decrease by the request count.
This diagnoses lost updates under bursts. It does not replace the separate
scarce-stock overselling check or measure sustainable throughput.

The draft `diagnostic-checkout-application-crash.json` and
`diagnostic-checkout-database-crash.json` scenarios also have zero points and are
not selected by campaigns. They require a disposable, owned lease and a
`--restart-spec` with the backend, app path, port and probe. Select **one feature**
with `--feature` on freshly reset reference data for each trial. Do not run the
whole file on shared data: an earlier cart reservation can expire during a later
trial. SpacetimeDB uses only the database scenario because its application logic
and database share one process boundary.

These probes kill owned processes with SIGKILL and restart them without resetting
storage. They record request outcomes, signal times and recovered business state.
Unconfirmed checkout effects may be absent or complete; partial effects and lost
confirmed state fail. SpacetimeDB calls use its native confirmed WebSocket protocol.
A separate checkout tests recovery progress. Application-crash recovery first
waits up to 70 seconds for the old database connections and transactions to end.
It records read-only observations and does not kill sessions or change timeouts.
If an HTTP call disconnects without proof that database work ended, stored-state
comparisons stay inconclusive until a fresh grade on reset data. A proven app
recovery failure still fails the recovery check when the crash window is valid.
A client timeout does not prove that server work stopped. Missed fault windows also remain
inconclusive. These are process-crash tests, not power-loss tests or proof of a
crash at a particular instruction inside a transaction. The diagnostics remain
draft and outside scored campaigns.

Run independent diagnostic cases through the reference command inside the Linux
appliance. Set `STACK_BENCH_CONTROLLER_IMAGE_ID` and `STACK_BENCH_IMAGE` to immutable
image IDs. No model credentials or paid calls are needed.

```sh
node dist/src/references/reference-live.js --diagnostic-plan /evidence/diagnostics.json --out /evidence/result.json
```

The plan selects existing zero-point scenarios and imported references:

```json
{
  "schemaVersion": 1,
  "groups": [{
    "backend": "postgres", "track": "ecommerce", "level": 3,
    "recipe": "ecommerce.progression-catalog",
    "scenario": "/workspace/tools/stack-bench/tracks/ecommerce/scenarios/diagnostic-checkout-database-crash.json",
    "features": [9800, 9803], "repetitions": 10
  }]
}
```

Each feature gets its own leased worker, source copy, database and ports. The
worker builds once and resets data between repetitions. Host resource admission
controls startup. Use `--diagnostic-workers 1` for the same execution path in
serial, or set a per-command concurrency limit. There is no additional host cap.

For a disposable candidate, add `source: { "path": "...", "sha256": "..." }`.
Its dependency files and deployment metadata must match the imported reference.
Declare exact criterion IDs in `expectedFailures` for defect controls. Source
paths and scenario paths are relative to the plan. Candidate source is copied;
the supplied tree is never edited.

For an accepted saved L3 app, use `saved` instead of `source`:

```json
{
  "run": "/evidence/attempt/run.json", "runSha256": "<sha256>",
  "checkpoint": 11, "source": "/evidence/accepted-source",
  "reader": { "path": "/evidence/reader.json", "sha256": "<sha256>" }
}
```

This path requires the final accepted checkpoint, its source and selection hashes,
and the original build image, run index, and database or module address. It installs
the app's own dependencies and does not deploy a reference app. Saved SpacetimeDB
apps require `STACK_BENCH_RELEASE_DEPS_VOLUME`, initialized from the original backend
image with the existing `appliance/dependency-volume` command. The runner verifies
the mounted SDK and native binaries against that backend image's manifest before
app startup. This volume supplies stack artifacts, not the app's `node_modules`.
Each trusted reader JSON contains `sourceSha256` and a reviewed mapping:

- PostgreSQL: `sql` uses `:'account'` and `:'item'` in a read-only, repeatable-read transaction.
- MongoDB: `script` reads through `store` in an aborted snapshot transaction. It receives `account`, `item`, `key`, and `minor` helpers.
- SpacetimeDB: `tables` selects one native subscription snapshot; `convert(tables, account, item)` maps its rows.

Each mapping returns `accountMatches`, `itemMatches`, and `state`. Both counts must
equal one. Reads require the owned container. Mapping programs are trusted operator
code, never supplied by the tested app. SpacetimeDB connection hooks can change
state before a fresh subscription; disclose this limit when the app has such hooks.

Saved order-only state includes allocations and orphan counts. Map separate refund
records when present. Use `refundedMinor: null` when no order refund amount is stored.
These mappings cover selected accounting fields, not the entire database. They cannot
claim payment or reservation coverage. They support checkout and crash recovery;
direct-purchase histories and cancellation require separate qualified mappings.
Every new source needs a reviewed reader and deliberate defect controls before
running the audit. Saved-app failures are measured results, not expected-control
failures. All results remain zero-point diagnostics and leave prior scores intact.

The output retains planned, started, collected, interrupted and unstarted trials.
Collected includes inconclusive results; it does not mean qualified. Raw grades
retain setup, action and assertion timing. Worker audits add deployment, reset,
grade and cleanup time. Unexpected failures stop new trials. Active trials finish;
explicit cancellation stops owned processes and releases their leases.
`--diagnostic-resume /evidence/previous.json` with a new output resumes only whole
groups that were never dispatched, under the same plan and images. Interrupted
executions stay visible and require an explicit new study to rerun.

## Agent adapter contract

Register an agent in `src/agents/agent-adapters.ts`. The existing registry accepts
a Node entry point; a new provider does not need another runner or result format.
Use `AgentRequest` from `src/agents/agent-adapter-contract.ts` and
`ValidatedAgentResult` from `src/agents/agent-result-contract.ts` as the protocol.
The runner sends arguments without a shell. The final non-empty stdout
line must contain one result JSON object. Earlier lines can contain logs.

The request carries the selected model, mode, app directory, visible task and
guidance. Preserve them exactly. Do not expose grading definitions to the agent.
Declare the modes, credentials, network destinations and cost limits the adapter
actually supports. Registering an entry point requires rebuilding the release;
there is no runtime plugin loader. Adapter identities bind its entry-point bytes
and declared settings, including grading credentials. The release binds the
remaining source files.

The runner validates results and deducts reported cost from the attempt's shared
budget. Unsupported cost limits fail before launch. A paid adapter must also use
the existing appliance, credential and cost-receipt controls; declaring a native
cost limit is not proof that an external scaffold enforces it. A standalone agent
has a deadline. An authenticated campaign delegates that deadline to its
supervisor so time grants remain effective. Cancellation stops the owned process
tree and group. Captured output is limited to 64 MiB per stream; exceeding that
limit rejects the result.

`tests/agent-adapters.test.ts` sends a compiled visible task to an independent,
model-free entry point in all four modes. It checks exact delivery, shared budget
accounting and process cleanup. These tests qualify the protocol boundary, not a
new provider's billing integration or a complete install-to-run walkthrough.

## Generated files

Run `npm run graph` to rebuild `docs/dependency-graph.html` from the versioned
ecommerce graph. Do not edit generated output by hand.

Build output, run artifacts, transcripts, local plans, and operational notes are
not product documentation and must remain untracked.
