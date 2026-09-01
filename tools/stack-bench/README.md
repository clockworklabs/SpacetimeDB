# Stack Bench

Stack Bench compares how coding agents build the same application with different
technology stacks. It builds each app in an isolated container, tests real user
behavior, gives failed checks back to the agent for bounded repairs, and keeps
the evidence needed to explain every result.

A campaign records the exact:

- product request and features;
- model and agent adapter;
- stack and stack guidance;
- checks and scoring rules;
- repair budget, repetitions, and parallelism;
- source, image, and dependency identities.

This records the inputs needed to compare compatible attempts by score, cost,
duration, and repair effort.

## How it works

1. **Define.** A versioned campaign file selects the work, model, stacks, checks,
   budgets, and attempt count.
2. **Validate.** Stack Bench compiles the file and shows the exact plan without
   calling a model.
3. **Preflight.** It checks credentials, images, ports, resources, dependencies,
   and persistent storage before model work starts.
4. **Build.** The coding agent receives the product request, current features,
   and selected stack material in an isolated container.
5. **Grade.** The grader drives separate browser sessions and stack operations.
   Each check produces a pass, failure, inconclusive result, or harness failure.
6. **Repair.** Conclusive app failures can be returned to the coding agent within
   the campaign budget.
7. **Report.** Stack Bench builds a comparison from the retained run evidence.

The coding agent does not receive the grader, checks, future work, scores, or
comparison data.

## Run modes

Stack Bench supports two ways to order product work:

- **Sequential:** complete each selected level before starting the next. Earlier
  checks run again to catch regressions.
- **Dependency:** each feature opens after its required parent features pass. A
  failed branch can stop while unrelated branches continue.

In dependency mode, a strike is spent only when a conclusive grade fails the
feature in the current coding request. Provider, harness, and inconclusive
results do not spend strikes. Repairs target one failed feature by default.

## What a run produces

Each attempt keeps:

- `run.json` with the exact inputs, outcome, usage, cost, and timing;
- grading artifacts with every suite and check result;
- contract and preflight evidence;
- source checkpoints and dependency progress;
- screenshots, videos, and traces when media recording is enabled;
- the repair reports sent to the coding agent.

A campaign report summarizes only compatible, verified attempts. Invalid and
incomplete attempts stay visible and do not become comparison data.

## Supported deployment

The supported v1 deployment is the Docker appliance on a dedicated Linux/amd64
runner. The controller has access to the Docker socket, so use a machine with no
unrelated workloads or credentials.

Start here:

1. Follow [SETUP.md](SETUP.md) for prerequisites and credentials.
2. Follow [appliance/README.md](appliance/README.md) to build, configure, and run
   the appliance.
3. Read [SYSTEM-DESIGN.md](SYSTEM-DESIGN.md) for the control flow and ownership
   rules.

Paid and subscription-backed model work runs only through the appliance. Local
source commands accept non-billable adapters for development and qualification.

## Campaign control

From `tools/stack-bench`, these commands inspect and prepare a campaign without
starting model work:

```bash
npm run campaign -- validate <campaign.json>
npm run campaign -- show <campaign.json>
npm run campaign -- prepare <campaign.json> --out <campaign-directory>
npm run campaign -- status <campaign-directory>
npm run campaign -- inspect <campaign-directory>
npm run campaign -- report <campaign-directory>
```

The CLI and optional dashboard use the same campaign plan, scheduler, and durable
state. See [dashboard/README.md](dashboard/README.md) for the web interface.

## Local development

Requirements: Node.js 22 or newer, Docker with Compose v2, and Chromium installed
through Playwright.

```bash
cd tools/stack-bench
npm ci
npm run bootstrap:browsers
npm run typecheck
npm test
```

Use the smallest check that covers a change:

| Change | Check |
|---|---|
| Prompt composition | `npm run check:prompts` |
| Track scenarios | `npm run check:scenarios` |
| Composition files | `npm run check:composition` |
| Dependency graph | `npm run graph` |
| Shared TypeScript | `npm run lint && npm run typecheck && npm test` |

Run Docker qualification only when the changed code affects the evidence it
proves. Run targeted mutations while developing checks. The complete mutation
set is a release-candidate gate.

## Project map

| Path | Owner |
|---|---|
| `tracks/` | product requests, feature definitions, checks, and scenarios |
| `conditions/` | prompt guidance and repair policy |
| `backends/` | stack-specific material sent to the coding agent |
| `commands/` | command-line entry points |
| `src/` | campaign, runtime, progression, evidence, and stack logic |
| `grader/` | grading and mutation execution |
| `linter/` | generated-app contract checks |
| `reference-apps/` | known-good applications used to qualify checks |
| `qualification-evidence/` | evidence bound to promoted definitions |
| `appliance/` | dedicated-runner packaging and operation |
| `dashboard/` | optional campaign interface |
| `docs/` | maintained diagrams and presentations |

Application-specific work belongs in a track. Runtime integration belongs in a
stack adapter. Prompt selection and scoring selection remain separate, so a
behavior can be measured without being named in the product request.

## More documentation

- [APPLIANCE-DESIGN.md](APPLIANCE-DESIGN.md): security and execution boundaries
- [appliance/README.md](appliance/README.md): appliance commands and recovery
- [grader/README.md](grader/README.md): checks, evidence, and grading
- [reference-apps/README.md](reference-apps/README.md): reference qualification
- [tracks/ecommerce/composition/README.md](tracks/ecommerce/composition/README.md):
  packs and recipes
- [docs/dependency-graph.html](docs/dependency-graph.html): generated ecommerce
  feature graph
- [docs/technical-guide.html](docs/technical-guide.html): current run path

Working notes, generated findings, transcripts, and run artifacts are local
operator data. They are not tracked as product documentation.
