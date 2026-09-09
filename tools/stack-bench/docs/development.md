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
docker build -t stack-bench-build:2.1.226 container
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
disposable L2 app copy with its authenticated lease environment, use the existing
grader entry point:

```sh
node dist/grader/grade.js --backend <stack> --url <app-url> --level 2 --spec tracks/ecommerce/scenarios/diagnostic-checkout-contention.json --out <diagnostic-result.json>
```

This standalone command deliberately omits `--track`. Both diagnostics include their named action mappings. Output is unbound to a recipe and has zero scored points; it cannot establish campaign completion. Backend reads still require the authenticated backend lease.

This starts no coding agent. It changes application data, so do not point it at a
campaign app or its retained database. Prepare each stack with the same runtime
and resources. The probe requires the declared stock data interface and two
sessions of each fresh test account. It sends 1, 4, 16, and 64 parallel checkout
requests, with three fresh-account cohorts per width. These are request counts,
not distinct client counts or sustained throughput.

Each cohort requires one order, one stored stock decrement, and an empty cart.
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

## Generated files

Run `npm run graph` to rebuild `docs/dependency-graph.html` from the versioned
ecommerce graph. Do not edit generated output by hand.

Build output, run artifacts, transcripts, local plans, and operational notes are
not product documentation and must remain untracked.
