# Development

This guide covers working on the StackBench source. Running the benchmark is
in the [appliance guide](../appliance/README.md).

## Set up

You need Node.js 22 or newer, the repository's pinned pnpm through Corepack,
Docker Engine with Compose v2, and Chromium through the pinned Playwright
dependency. Campaign and resource-lock tests use Linux `flock`, so on Windows
or macOS run them in the source container below.

```bash
pnpm --filter spacetimedb install --frozen-lockfile --ignore-scripts   # crates/bindings-typescript, the SDK
cd tools/stack-bench
npm ci
npm run bootstrap:browsers
docker build --platform linux/amd64 -t stack-bench-build:local container
```

The last command builds the build image used by local tests. Real stack
execution needs the appliance images from the
[appliance guide](../appliance/README.md#set-up-for-model-runs).

## Which check to run

Run the smallest check that covers the change. Every test and check script builds `dist`
first and a build clears it, so do not run two at once; to run several checks
against one build, run `npm run build` and then call `node --test dist/tests/...`
or `node dist/commands/...` directly.

| Change | Check | Runs |
|---|---|---|
| TypeScript | `npm run typecheck`, then the focused test | |
| Engine code | `npm test` | `tests/*.test.ts` |
| Dashboard read model, routes, pages | `npm run test:dashboard` | `tests/dashboard/*.test.ts`; writes thirty campaigns of fixture evidence |
| Tracks, prompts, references, repository policy, campaign definitions | `npm run test:contracts` | `tests/*.contract.ts` |
| Reference source, grading checks, seeded-bug manifests | `npm run test:mutation-definitions` | `tests/*.mutation.ts`; model-free |
| Browser, process, or Docker boundary | `npm run test:integration` | `tests/*.integration.ts`; serial, owns browsers, ports, and Docker |
| Prompt composition | `npm run check:prompts` | |
| Scenarios | `npm run check:scenarios` | |
| Packs and recipes | `npm run check:composition` | |
| Calibrations | `npm run check:calibration` | |
| Reference registry | `npm run check:references` | `reference-apps/registry.json` and each fixture's hash |
| Scenario, contract, reference, or SDK source behind a pinned hash | `npm run repin` | |
| Feature graph | `npm run graph` | |
| SpacetimeDB transport | `npm run build`, then `node --test dist/tests/spacetime-captured-replay.integration.js` | |
| Container preparation and reset publish | `npm run test:container` | needs Docker, the build image, and the local SpacetimeDB CLI |
| Teardown during a stack restart | `npm run test:faults` | same requirements |
| Whole bench loop with the deterministic agent | `npm run test:loop` | stub backend, `loop` track |

Once a shared runtime, composition, grading, campaign, or release change is
stable, run the source gate once:

```bash
npm run lint
npm run typecheck
npm run test:all
```

`test:all` runs the unit, dashboard, and contract tiers; integration and
mutation-definition tests stay separate. On a non-Linux host, run it in the
controller image's `source` target, which holds the compiled source and tests
without the Rust binaries or development dependencies, so run the compiled
tests directly:

```sh
docker build --platform linux/amd64 --target source -f tools/stack-bench/appliance/Controller.Dockerfile -t stack-bench-source-tests:local .
docker run --rm --init --network none stack-bench-source-tests:local node --test --test-concurrency=4 "dist/tests/*.test.js" "dist/tests/dashboard/*.test.js" "dist/tests/*.contract.js"
```

Browser tests under `tests/dashboard/*.browser.ts` run by hand with
`STACK_BENCH_BROWSER_TEST_URL` pointing at an isolated appliance.

A passing check stays valid until one of its inputs changes. Add a test only
for an invariant no existing test covers. Documentation changes need link and
formatting checks only.

## Pack budgets

A pack's runtime budget is the time its checks may take on a clean reference.
For a pack whose budget is `unmeasured`, collect timings with
`qualify-reference --timing-only`. Timing artifacts cannot qualify a release.
Then run `pack-budget recommend`. It sets the budget to twice the slowest
passing timing, rounded up to the next second, and never below the waits the
pack's checks declare. A pack that exceeds its budget fails, so set a growing
pack's budget back to `unmeasured` before timing it again. Then qualify the frozen candidate as usual with the
[qualify-checks skill](../skills/qualify-checks/SKILL.md).
