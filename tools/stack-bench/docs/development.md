# Stack Bench development

This guide covers local source development. Use the
[appliance guide](../appliance/README.md) for runner configuration, credentials,
preflight, campaigns, and paid model work.

## Requirements

- Node.js 22 or newer
- Docker Engine with Compose v2
- Chromium installed through the pinned Playwright dependency
- the repository Linux CLI and TypeScript bindings for SpacetimeDB work

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

On a Windows checkout, build the Linux SpacetimeDB CLI before testing the
SpacetimeDB adapter:

```bash
bash container/build-linux-cli.sh
```

The local image tag is for development. The appliance uses image digests from
its release manifest.

## Source checks

Run the smallest check that covers the change:

| Change | Check |
|---|---|
| TypeScript | `npm run typecheck` and the focused compiled test |
| Unit tests | `npm test` |
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

Use `npm test` while changing code. Run `npm run test:contracts` when tracks,
prompts, reference applications, repository policies, or campaign definitions
change. `npm run test:all` runs both tiers after one build. Docker and
qualification checks remain separate.
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

## Generated files

Run `npm run graph` to rebuild `docs/dependency-graph.html` from the versioned
ecommerce graph. Do not edit generated output by hand.

Build output, run artifacts, transcripts, local plans, and operational notes are
not product documentation and must remain untracked.
