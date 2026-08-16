# Stack Bench extensibility roadmap

Status: draft for implementation. The current ecommerce L1/L2 definitions are
useful calibration inputs, not permanent interfaces.

Ticket-sized dependencies, ownership lanes, and acceptance evidence are tracked
in [IMPLEMENTATION-PLAN.md](IMPLEMENTATION-PLAN.md). This roadmap remains the
architecture and product contract; the implementation plan is the execution
source of truth.

## V1 experiment brief

Stack Bench v1 exists to produce defensible, reproducible evidence of how
SpacetimeDB compares with MongoDB and PostgreSQL on the same application tasks.
Additional stacks and evolving benchmark recipes should be feasible later, but the v1
deliverable is the comparative experiment and its evidence—not a general
third-party benchmark platform or plugin marketplace.

It should reproduce the useful measurements of the sequential harness while
making isolation, grading, calibration, provenance, invalid-run handling, and
teardown explicit and repeatable. Internal extensibility matters because the
team has not frozen L1 or later levels and must be able to improve the experiment
without rewriting the instrument.

### V1 deliverables

1. **Experiment protocol** — declared recipes, stacks, agent/model/settings,
   run order, repetition policy, fix policy, scoring, exclusion rules, and
   analysis plan decided before the measured series.
2. **Qualified benchmark recipe** — exact pack and fixture versions plus scoring,
   canonical references, null controls, exact mutation controls, and repeated
   Docker qualification for SpacetimeDB, MongoDB, and PostgreSQL.
3. **Measured dataset** — every attempted run, including infrastructure-invalid
   attempts, with immutable provenance and raw evidence. Exclusions and reruns
   are additive records rather than deletion or replacement.
4. **Comparison report** — score, correctness, repair behavior, cost, duration,
   code/runtime metrics, variability, invalid-run rate, and evidence limitations
   presented without attributing differences the data cannot support.
5. **Reproduction bundle** — frozen source/commit, definitions, images and
   dependency identities, runner instructions, artifact checksums, and the code
   used to derive report tables from raw results.

### Reproducible experiment design

Matching source hashes is necessary but insufficient. Each experiment records
or controls:

- agent/provider/model identity and all behavior-affecting settings;
- exact prompt/task bytes, fix feedback, recipe, packs, and fixtures;
- prompt-cache policy and whether a run could benefit from a warm prefix;
- engine, stack adapter, recipe, packs, fixtures, calibration, and artifact versions;
- source commit/dirty state and every mounted artifact hash;
- container image digests, OS/architecture, browser, runtime, CLI, SDK, and
  dependency-lock identities;
- run order, repetition, seed, timestamps, host resource observations, retries,
  checkpoints, exclusions, and operator interventions;
- phase deadlines, budget violations, teardown outcome, and invalidity cause.

Use seeded randomized or counterbalanced stack order so sequential execution
does not systematically give later stacks warmer caches or a different host
state. Infrastructure-invalid runs remain first-class attempts and are never
silently retried out of the dataset. The analysis reports dispersion and sample
size; a single run is not used to make cost or reliability claims.

### V1 acceptance

V1 evidence is ready for external review only when:

- the protocol and analysis plan are frozen before the measured series;
- all three canonical references pass the same frozen engine/recipe image
  hashes in repeated Docker runs, including null, mutation, and teardown gates;
- the measured agent series completes the declared repetitions and ordering, or
  reports every incomplete/invalid attempt transparently;
- all result bundles contain sufficient provenance to detect any comparison-
  defining drift;
- automated report generation can be reproduced from raw immutable artifacts;
- findings distinguish application failures, inconclusive evidence, harness
  failures, and product friction;
- limitations include sample size, host pressure, evidence-channel differences,
  and any criterion that is not equally observable across stacks;
- a clean supported runner can reproduce a small subset from the documented
  protocol without relying on unrecorded machine state;
- artifact checksums and the exact report-producing code accompany delivery.

A generic non-Claude adapter ecosystem, public plugin distribution, and a
polished third-party SDK are useful future options, not v1 blockers. The engine
should avoid making them impossible, but v1 scope is the trustworthy three-stack
comparison.

### External delivery and operator experience

An external evaluator must be able to run the same experiment without this
repository's ambient developer state. The preferred v1 delivery is a signed,
content-addressed OCI bundle published through Harbor (or an equivalent OCI
registry):

- Stack Bench controller/runner image;
- build-sandbox image;
- pinned SpacetimeDB, PostgreSQL, and MongoDB service images;
- versioned benchmark and calibration pack;
- experiment manifest and Compose bundle;
- provider credential/secrets template with no embedded credentials;
- SBOMs, signatures, image digests, source revision, and checksum manifest;
- persistent results volume and deterministic JSON/static-HTML report tooling;
- concise operator guide, troubleshooting/runbook, and support/version policy.

The target workflow is small and non-interactive after credentials are supplied:

```text
docker login <registry>
docker compose pull
docker compose run --rm stack-bench preflight
docker compose run --rm stack-bench qualify
docker compose run --rm stack-bench run --experiment stdb-vs-pg-vs-mongo-v1
docker compose run --rm stack-bench report
```

Command names may change; the preflight, qualification, experiment, and report
boundaries may not. `preflight` must detect architecture, Docker/resources, image
digests, credentials, outbound provider access, ports, filesystem capacity, and
unsupported host state before a paid run.

The controller currently needs to create and identify per-run containers. A
mounted Docker socket is root-equivalent access to the host, not a routine file
mount. V1 therefore supports the first topology only; the exact filesystem,
network, dependency-volume, and secret boundaries are specified in
`APPLIANCE-DESIGN.md`:

1. a dedicated disposable Linux x86-64 VM/runner with tightly scoped registry
   and LLM credentials (**selected for v1**);
2. a proven rootless nested container runtime; or
3. a future remote worker/orchestrator API that keeps the controller away from
   the host socket.

Do not advertise general workstation installation until its threat model and
cleanup behavior are qualified. Linux x86-64 can be the first supported runner
if declared explicitly; other platforms require their own acceptance evidence.

External handoff acceptance additionally requires:

- a clean dedicated runner can authenticate, pull by digest, run `preflight`, and
  complete a small no-model smoke test from only the delivered bundle;
- no host source checkout, developer home-directory credential, or mutable image
  tag is required;
- interrupted runs preserve evidence and either clean exact resources or emit a
  quarantine/recovery instruction;
- raw artifacts and the rendered report survive controller/container removal;
- all outbound network destinations and secret mounts are documented and
  minimized;
- delivered checksums/signatures verify before execution and again in result
  provenance;
- a second clean runner reproduces the reference qualification and report from
  the same bundle.

### Agent/model boundary

V1 must not embed Claude semantics in the experiment engine even if Claude is
the first and only qualified live provider. Define and contract-test a versioned
agent adapter with:

- task/prompt input, workspace, allowed tools, environment, deadline, and
  cancellation;
- build, upgrade, and repair session modes;
- normalized completion state, usage, duration, and transcript reference;
- raw provider-specific metadata retained without forcing false equivalence;
- model/provider identity, reasoning settings, context/tool limits, cache
  behavior, authentication mode, and pricing-snapshot provenance;
- bounded diagnostics, redaction, and cleanup behavior.

Ship the current Claude implementation behind that contract plus a deterministic
fake/fault adapter for lifecycle tests. Qualify additional providers
incrementally. A future model-by-stack study is a separate experiment matrix:
counterbalance model and stack ordering, record tool-capability differences,
and do not compare token counts or dollar cost across providers without an
explicit normalization and pricing policy.

## Outcome

L1 and L2 remain useful human-facing labels, but they are not permanent engine
concepts. A published label resolves to a versioned recipe assembled from
reusable behavior-sized test packs. Changing L1 means publishing a new recipe
version; mixing a different set of packs means publishing a different recipe.
Neither operation should require engine code.

Adding a new test with existing actions should normally mean editing one pack.
Adding a genuinely new action should require one small plugin, its schema, and
focused contract tests—not edits across the main grader, reporting, redaction,
and every backend.

The production system must keep the safeguards already earned through live
failures: Docker-only execution, exact resource leases, bounded phases,
fail-closed cleanup, immutable provenance, explicit inconclusive evidence, null
controls, mutation controls, and repeated cross-backend reference qualification.

## Current extensibility audit

The current system is reasonably efficient for adding a criterion that uses one
of the 44 step types exercised by current scenarios. The legacy grader implements
47 in total. It is not yet production-safe for adding a new
action or changing what a level means.

Verified strengths:

- tracks declare suites as data and an undeclared level is rejected rather than
  falling back to L1 (`tracks.mjs`);
- later levels can deliberately rerun earlier guarantees;
- scenario checks cover known actions, actors, hooks, points, and prompt grounding;
- rubric identity and exact scenario bytes already receive separate hashes;
- mutation analysis separates target failures, collateral, setup failure,
  missing evidence, and inconclusive evidence;
- reference qualification requires repeated runs with the same image and
  executable harness hash.

First compatibility slice completed:

- The definition compiler now normalizes current track/scenario inputs into
  definition schema v1 and validates the full action field shape before browser
  or backend work begins.
- The 47-action runtime vocabulary is explicit. The checker consumes that
  registry, and a compatibility test separately proves it remains equal to the
  legacy grader dispatcher while extraction is underway.
- Tracks and named actions now pass through one fail-closed manifest compiler;
  malformed named actions can no longer silently become an empty set.
- Scenario level semantics come from the declared level rather than a filename
  prefix.
- Public result artifacts now use artifact schema v2: a strict common envelope,
  per-kind payload fields, exact available identities, parent-attempt ancestry,
  and timestamps. Readers preserve historical v0/v1 evidence without rewriting
  it and reject unknown versions and explicit unknown kinds.
- Active track manifests now declare suite inheritance explicitly. The compiler
  preserves name-based legacy-v0 behavior only at its compatibility boundary;
  runtime suite selection no longer guesses semantics from suite names.
- Canonical, human-diffable golden plans now freeze the compiled semantics of
  every active track and a compatibility definition covering all 47 actions.
  Drift checking is read-only unless an author explicitly accepts an update.
- The validated ecommerce L1/L2 workload is now expressed as eight behavior
  pack identities, two versioned fixture sets, exact parity recipes, and a
  separately versioned promotion catalog. Compilation proves the same
  suite/feature/check order and 51-point L1/75-point aggregate L2 scoring. A
  smaller smoke recipe proves pack reuse with explicit recipe-owned weights.
- Recipe releases now carry deterministic meaning, execution, and combined
  content hashes plus exact source digests and a compact stable-key catalog.
  Ecommerce L1/L2 grades and bundles record the alias and exact resolved
  identity after proving the recipe matches the active execution plan.
- Framework-neutral L1 1.1 and L2 1.2 are qualified and promoted with repeated
  Docker reference, mutation, and null evidence for all three stacks. Each has
  all 13 required evidence slots hash-bound and zero machine qualification
  blockers. The prior L1 1.0 and L2 1.1 releases are retired; their qualified
  calibration records remain valid evidence but cannot be selected for a new
  run. One exact `--recipe` selector runs any non-retired catalogued release
  through the normal runtime and qualification paths without moving an alias.
- Current run, grade, bundle, lint, action, mutation, null, reference,
  performance, bug-quality, and public lease-evidence producers use the common
  artifact writer. Runtime reporting and qualification readers accept only the
  current strict schema; pre-v1 bytes are an inert checksummed archive. The deterministic orchestration loop verifies
  real parent chains and secret exclusion; Docker smoke and fault injection
  verify the container lifecycle boundary.
- Check evidence schema v1 now carries a typed verdict, machine code, phase,
  actor, observation/expectation, retryability, timing, attachment references,
  sensitivity labels, and the underlying versioned action evidence. Setup
  evidence is stored once per feature and setup-blocked criteria reference it.
  Artifact validation rejects contradictions, while historical boolean and
  prose-prefix artifacts are rejected by active readers. Scoring,
  comparisons, null and mutation controls, run outcomes, reference qualification,
  and repair selection no longer derive meaning from diagnostic wording.
- Stack adapter schema v1 now provides strict static registration and versioned,
  named capability providers for all three measured stacks and the deterministic
  offline stack. Ports, leases, reset, lifecycle, diagnostics, grading context,
  named transport, direct database writes, build-container isolation, reference
  deployment, measurement conventions, and teardown are adapter-owned. Required
  operations and provider identities are checked at startup; a complete fake
  stack registers without runner, grader, scoring, or reporting edits.
- Read-only `pack validate`, `recipe validate`, `recipe show`, and `recipe diff`
  commands now resolve the full source context without Docker. Diff output
  separates meaning, scoring, fixture, execution, and metadata changes and
  names the exact matching calibration bindings and repetitions invalidated.
  Pack/check filters produce an exact selection identity tied to the source
  recipe hash and now drive the compatibility runner by stable check key.
  Requests, resolved checks, attempted checks, and explicit not-run reasons are
  durable evidence. Subsets are legitimate scopes, while direct comparison is
  refused unless recipe and selection hashes match.
- The schema-v1 registered-action contract and compatibility catalog now cover
  all 47 current actions. Production startup validates unique registrations,
  exact input compilers, capability declarations, hard deadlines, evidence
  types, redaction tags, renderer metadata, and narrow executor inputs. A fake
  runtime proves pass, application failure, inconclusive, cancellation,
  deadline, malformed evidence, and unclassified-exception behavior. The
  all 47 actions now execute through independent registered implementations and
  narrow capabilities. Duplicate grader branches and the central compatibility
  switch are gone. Concurrency recursively uses the same registered dispatch;
  database and lifecycle differences are capability providers outside browser
  grading.

Verified debt still to remove:

- Active scenario sources still hold the executable check bodies while packs
  reference them through a strict compatibility boundary. The compatibility
  runner now binds those sources to exact recipe identities and selected scopes,
  but a general non-legacy recipe executor remains future work.
- Pack-owned requirement and testing-hook fragments now compose the builder
  task from the same selection that owns checks. Full ecommerce L1/L2 task text
  remains byte-identical, and session durability is independently selectable.
  Chat still needs migration into this pack/recipe authoring model.
- Human descriptions and feature orchestration remain coupled to the grader,
  but all action execution now has an isolated contract-test seam. Humanisation
  and redaction still use regex-based rendering rules even though classification
  itself consumes typed evidence.
- Experiment identity is explicitly null until experiment-plan execution lands;
  stack-adapter identity is currently the stack id plus the encompassing engine
  hash until adapters become separately packaged releases.
- Calibration lifecycle now gives every zero-point criterion a typed policy.
  The promoted L1/L2 releases are qualified; each new exact recipe version must
  collect fresh evidence before its alias can move.
- Agent adapter schema v1 now gives the engine one strict build/upgrade/fix
  request and normalized completion, usage, duration, setup, and transcript
  boundary. The current Claude Code implementation, deterministic loop fixture,
  lifecycle fault injector, and model-free reference deployer are statically
  registered. Provider-specific container commands, credentials, cache behavior,
  pricing interpretation, and raw transcript collection remain inside the
  provider implementation rather than the campaign engine.

Treat the current level/suite/scenario format as a compatibility input while its
behavior moves into packs and recipes. Historical result bytes are preserved in
a checksummed archive, but the active artifact reader accepts only the current
schema. We do not rewrite old evidence or pretend it was authored in the new model.

## Product composition model

### Test packs

A test pack is the smallest behavior area worth selecting, understanding, and
calibrating as a unit. It contains several related checks, including ordinary
behavior, invariants, concurrency, recovery, or security checks when they all
protect the same product claim. Packs are organized by behavior—not by testing
technique.

The initial ecommerce pack boundaries should be approximately:

- identity and access;
- catalog and inventory;
- cart;
- checkout;
- realtime consistency;
- recovery and restart;
- security boundaries.

These are initial authoring boundaries, not immutable names. Split a pack when
its parts have different setup, meaning, ownership, or useful selection. Keep
checks together when selecting only half would make the claimed behavior
misleading. Avoid both one-pack-per-check fragmentation and one-pack-per-level
coupling.

Each pack declares:

- permanent pack and check IDs;
- a human version and lifecycle state: `draft`, `qualified`, or `retired`;
- ordered public requirement and testing-hook fragments plus pass/fail
  assertions, but not study-specific weights;
- required fixtures, actors, capabilities, setup and runtime budget;
- actions, evidence requirements, null expectations, and mutation targets;
- pack dependencies, conflicts, and values it provides to other packs;
- separate meaning and execution fingerprints.

An ID is never reused for a different claim. Retired versions remain readable.
A pack version may be qualified individually, but that does not qualify every
recipe containing it.

### Recipes

A recipe selects exact pack versions, assigns scoring weights, supplies global
task framing, and names the resulting benchmark. Packs supply the selectable
public requirements and hook contracts. `L1` and `L2` are aliases
to specific promoted recipes, not special branches inside the grader.

Recipes may freely mix compatible packs during development. A compiled recipe
must reject missing dependencies, duplicate checks, conflicting setup, cycles,
unsupported capability requirements, and ambiguous scoring. Development output
is marked `experimental`. A recipe becomes publishable only after that exact
combination passes reference, null, mutation, and teardown qualification on all
supported stacks.

Scoring belongs to the recipe so the same behavioral pack can be reused in a
standard or focused study without copying its checks. Reports always show raw
check outcomes as well as recipe-weighted scores.

Aliases such as L1 are entries in a versioned promotion catalog, not part of an
immutable recipe's identity. A run invoked through an alias records both the
alias/catalog version and the exact resolved recipe hash, so moving L1 later
cannot change what an old result means.

### Fixtures

Starting users, products, stock, rooms, permissions, clocks, and external stubs
are a separately versioned fixture set. A fixture change can alter what a test
detects even when its prose and actions are unchanged, so the exact fixture
identity is part of the compiled recipe and every result.

### Meaning versus execution changes

Every pack and compiled recipe has two fingerprints:

- **meaning** — requirement text, assertions, stable check IDs, and scoring;
- **execution** — selectors, timing, actions, probes, fixtures, and other test
  mechanics.

Any change produces a new exact content hash and requires requalification. A
meaning change always creates a new comparison cohort. An execution-only fix may
remain comparable only after an explicit, recorded equivalence decision backed
by calibration evidence; equivalence is never inferred automatically.

Human semantic versions are navigation aids, not proof of comparability:

- an execution-only correction is normally a patch release candidate;
- adding a new claim/check is normally a minor release and changes recipe
  meaning for recipes that adopt it;
- removing a claim, changing an existing claim's meaning, or restructuring the
  pack contract requires a major release or a new permanent check ID;
- changing recipe membership, instructions, or weights changes recipe meaning;
- any fixture change creates a new fixture version and execution fingerprint;
- an L1/L2 alias moves only after the new exact recipe is qualified.

The exact hashes and recorded qualification—not the version number—decide
whether results may be compared.

### Experiment plans

A recipe states what is tested. An experiment plan independently states how the
study is run: stacks, models, settings, repetitions, ordering, budgets, retries,
exclusions, pricing snapshot, and analysis policy. Changing the model therefore
creates a new experiment—not a new L1 recipe.

### Modular prompt builder and grading treatments

The product needs a real prompt builder, not hard-coded prompt modes. It treats
these choices as independent, versioned inputs:

1. **Feature modules** - visible product capabilities such as accounts, catalog,
   cart, checkout, reviews, fulfilment, or returns.
2. **Specification modules and treatments** - guarantees such as authorization,
   durability, reconnect recovery, atomicity, accounting integrity, concurrency
   safety, or lifecycle recovery. Every selected specification has exactly one
   treatment.
3. **Backend** - the required data platform, independently of implementation
   libraries or architecture.
4. **Backend-guidance profile** - access facts, API reference, and optionally
   framework, ORM, transport, layout, or architecture advice.
5. **Repair policy and budget** - which scored evidence may be returned to the
   agent and how many correction rounds may run.

The closed treatment set keeps disclosure, scoring, and repair behavior explicit
without allowing contradictory boolean combinations:

| treatment | in initial prompt | counts toward score | eligible for repair |
|---|---:|---:|---:|
| requested | yes | yes | yes |
| expected | no | yes | yes |
| observed | no | no; separate diagnostic | no |
| excluded | no | no | no |

The compiler assembles the exact prompt from selected feature fragments,
requested specification fragments, selected stack material, and only the
minimum public testing interface needed by the core product. The run records
every module ID/version and treatment, every rendered fragment ID/hash, the
scored and observed check sets, and the final prompt hash. Adding a module changes
data and qualification evidence; it does not add a branch to the runner.

Feature and specification modules are deliberately different. A feature module
owns product-facing request text and ordinary functional checks. A specification
module owns guarantee text plus a requested observation and, where possible, a
public-surface unmentioned observation. The unmentioned observation can be used
as either expected (scored and repairable) or observed (separate and
non-repairable). A condition can therefore request `cart + checkout`, request
`authorization`, expect `durability`, observe `concurrency`, select `postgres`,
and choose neutral backend guidance without editing runner code or creating a
monolithic prompt file.

Prescribed, neutral, and defaults-style studies are saved combinations, not
engine branches. A study condition binds exact feature, specification-treatment,
backend-guidance, repair, and prompt identities. Operators may create a new
combination from registered modules without changing `bench.mjs` or campaign
scheduling.

The backend guidance contract distinguishes **access material** from **design
advice**. Access material is the minimum needed to use the selected backend in
the isolated runner: exact connection/module coordinates, required ports,
available SDK/CLI locations, and selected official reference documents. Design
advice names frameworks, ORMs, polling/subscription patterns, transaction or
locking strategies, directory layout, or implementation recipes. Neutral and
defaults profiles may include access material but must exclude design advice.
SpacetimeDB, MongoDB, and PostgreSQL need symmetric neutral profiles; a stack
that lacks one makes that study condition fail compilation rather than silently
fall back to prescribed guidance.

Scored and observed evidence remain different classes:

- requested and expected checks retain recipe weights, affect pass/fail, and may
  produce a sanitized repair report;
- expected requirements are absent from the initial prompt, so their first-build
  score measures what the model and stack supplied without being told;
- observed checks have no score contribution, never affect run outcome, and
  never appear in repair prompts;
- observed checks run against the immutable first-build source before any repair
  and record that source hash;
- a repaired app cannot replace the untouched first-build observation;
- reports state requested, expected, observed, and excluded scope explicitly and
  keep observed-only denominators separate.

A specification such as durability has one versioned semantic definition, with
requested and unmentioned observation contracts when their interfaces differ.
The unmentioned contract must work without any selector or implementation detail
that was withheld from the model. It observes the public surface required by the
core task, a stack-neutral named action, or grader-owned database/runtime truth.
The compiler rejects expected or observed treatment when no such observation
exists. This avoids copying durability into multiple modules while preventing a
withheld requirement from assuming a testing hook it never requested.

The build container receives only requested prompt material and selected backend
access material. It does not receive expected or observed specification IDs,
definitions, selectors, expected values, grader code, or future repair reports.
After scored first-build grading, failures from requested and expected checks may
enter the sanitized bug report. Observed-only evidence remains outside that
directory and cannot enter repairs.

The campaign schema should evolve from one `guidance` string to content-bound
condition identities, approximately:

```json
{
  "condition": {
    "recipe": "ecommerce.l1-standard@2.0.0",
    "features": ["accounts", "catalog", "cart", "checkout", "reviews"],
    "specifications": {
      "requested": ["authorization@1.0.0"],
      "expected": ["durability@1.0.0"],
      "observed": ["concurrency@1.0.0"]
    },
    "guidanceProfile": "neutral-backend-access@1.0.0",
    "repairPolicy": "scored-only@1.0.0"
  }
}
```

The compiler resolves this into exact prompt bytes, requested feature and
specification fragments, scored checks, isolated observed checks, selected
access documents, repair eligibility, and all
content hashes. `run.json`, campaign plans, comparison admission, and reports
bind those resolved identities. Results are comparable only when the complete
condition identity matches; comparing prescribed with neutral is an explicit
factor in an experiment, never an accidental cohort merge.

Production acceptance for this model requires:

- a fake stack can add a neutral guidance profile without engine edits;
- a fake feature or specification module can be registered without runner/report edits;
- changing a specification treatment changes prompt, score, repair, and
  observation scope exactly as the treatment table declares without changing
  feature selection;
- all three measured stacks compile and run the same neutral/defaults condition;
- prompt snapshots prove withheld prose, check IDs, selectors, and mechanics are
  absent from the build container;
- public test-interface fragments are neutral descriptions of observable
  controls; they cannot restate a withheld guarantee;
- repair fixtures prove expected failures enter bug reports while observed
  failures never do;
- first-build source hashes bind every observed-only artifact;
- null, reference, and mutation controls qualify each unmentioned oracle on every
  supported stack;
- report tests prove expected checks count in scored results while observed-only
  results cannot be summed, averaged into score, or mislabeled as corrected;
- campaign validation rejects missing, mutable, unsupported, or contradictory
  condition/profile identities;
- active readers require the new schema for new campaigns; old result bytes stay
  in the inert archive and are not upgraded through a compatibility layer.

## Target model

Version these independently:

1. **Engine** — lifecycle, leases, Docker, browser orchestration, action
   execution, evidence storage, scoring, and failure classification.
2. **Test packs** — reusable behavior checks and their evidence requirements.
3. **Recipe** — exact pack versions, task instructions, scoring, and capability
   requirements.
4. **Guidance and treatment policy** — backend access and design advice,
   specification disclosure/scoring treatment, observability contracts, and
   repair eligibility.
5. **Fixture set** — exact initial data and controlled external dependencies.
6. **Calibration and promotion** — canonical references, null expectations,
   mutation targets, equivalence decisions, qualification policy, lifecycle,
   and aliases such as L1/L2 for exact recipes.
7. **Experiment plan** — condition matrix, stacks, models, repetitions,
   ordering, budgets, and analysis policy.
8. **Artifact schema** — one strict current result envelope; pre-v1 bytes remain
   in a checksummed, non-executable archive.

Every current result records these identities and hashes. A meaning change starts
a new comparison cohort. Archived pre-v1 bytes remain manually inspectable, but
cannot silently enter a current comparison.

## Authoring experience

### Add a check with existing actions

1. Scaffold a check into a chosen pack version.
2. Select typed actions and assertions from generated documentation.
3. Validate IDs, references, required capabilities, and step schemas locally.
4. Preview the recipe scoring and the exact evidence the check will emit.
5. Run focused reference and null checks.
6. Add a mutation target when the check protects a material invariant.

No engine edit should be needed.

### Change L1 or mix packs

1. Fork the current recipe or create a focused recipe.
2. Select exact pack and fixture versions and assign recipe weights.
3. Run `recipe diff` to separate meaning, scoring, fixture, and execution changes.
4. Generate the invalidated pack/reference/null/mutation checklist.
5. Qualify the exact compiled combination, then promote it explicitly and move
   the L1 alias if desired.

Old cohorts remain queryable and are never compared to the new version without
an explicit normalization policy.

### Add an action

Implement one registered action plugin containing:

- a unique, versioned action name;
- runtime input schema and generated authoring type;
- declared capability requirements;
- an executor receiving only a narrow run context;
- a structured evidence result schema;
- redaction and human-rendering metadata;
- deadline/resource budget;
- focused unit, contract, replay, and failure-classification tests.

## Architecture boundaries

### Definition compiler

Compile human-authored packs, recipes, fixture manifests, and legacy track/level
inputs into one normalized execution plan. Compilation must fail on unknown
fields, duplicate IDs, dangling references, dependency cycles, conflicts,
invalid scoring, unsupported action versions, or impossible capability
requirements. Runtime execution consumes only the compiled plan.

The compiled form carries stable check keys and recipe fingerprints. It is
stored with the result so the run remains understandable if source definitions
later move.

### Action registry

Replace the central action switch with a registry keyed by action name and
version. Plugins must not receive the entire mutable grader context. Provide
narrow services instead: actors, browser observations, direct database probes,
backend lifecycle control, timing/barriers, artifact sink, and cancellation.

Registration is explicit. Unknown and duplicate actions are startup errors.
Plugins cannot introduce free-form shell execution into promotable definitions.

### Structured evidence

Actions and assertions return data, not preformatted prose:

- status: passed, failed, inconclusive, or harness failure;
- machine-readable observation and expected value;
- actor/backend/timing metadata where relevant;
- bounded diagnostic attachments;
- sensitivity/redaction tags;
- causal phase and retryability.

Scoring, console output, bug reports, mutation analysis, and artifact rendering
must consume this same evidence. This prevents one failure from acquiring
different meanings in different reports.

### Capability registry

Each backend adapter advertises versioned capabilities. Packs declare what a
check requires. Compilation/preflight determines whether the chosen
backend can supply conclusive evidence.

Unsupported capability is never a pass. Promotion policy decides whether it is
allowed, zero-point, or blocks the recipe for that backend.

### First-class concurrency

Provide one workload primitive for contention tests:

- actor population and authentication state;
- readiness/actionability barrier;
- dispatch operation and maximum dispatch window;
- accepted/rejected accounting;
- postconditions and observation barrier;
- structured per-actor failure evidence.

Browser clicks, HTTP requests, reducers, and future transports should be
adapters beneath this primitive rather than separate scenario conventions.

### Replay

Persist sanitized action inputs, ordering, barriers, observations, and evidence.
Pure assertion/scoring/reporting logic must be replayable without Docker or a
browser. Operations that truly require a live subject are marked non-replayable
and retain their bounded raw attachments.

Replay is a debugging and compatibility tool, not a substitute for live
qualification.

### Artifact boundary

Use an explicit artifact `schemaVersion` and accept only the active schema in
production readers. Preserve pre-v1 bytes unchanged in a checksummed inert
archive; do not infer missing identities, migrate verdict meaning, or present
historical evidence as current qualification.

## Delivery plan

### Phase 0 — characterize and freeze behavior

- Inventory every current action, assertion, backend branch, output shape,
  redaction path, and failure classification.
- Capture representative current scenario plans and result artifacts as golden
  compatibility fixtures.
- Add tests proving current unknown-action/unknown-field behavior before it is
  changed.
- Document which current behaviors are intentional and which are legacy debt.

Exit gate: the existing suite, null controls, mutation controls, and canonical
L1 references remain green with no scoring or evidence drift.

### Phase 1 — versioned definition and artifact schemas

- Define runtime schemas for packs, recipes, fixtures, actions, calibration,
  experiment plans, and the result envelope.
- Add a compatibility compiler that maps current levels/suites/scenarios into
  packs and recipes without changing execution.
- Compile exact pack versions, fixture identity, prompt bytes, scoring, and
  capabilities into one recipe release with meaning and execution fingerprints.
- Record engine, pack, recipe, fixture, calibration, experiment, adapter, and
  artifact identities in every result.
- Add `pack validate`, `recipe validate`, `recipe show`, and `recipe diff`.

Exit gate: current definitions compile byte-for-byte deterministically into the
compatibility model; malformed, conflicting, incomplete, or ambiguous recipes
fail before any backend resource is acquired; old artifacts remain verifiably
archived and fail closed in active readers.

### Phase 2 — action registry behind the current engine

- Introduce registry and plugin contracts.
- Move low-risk pure actions first, then browser observations, direct database
  writes, concurrency, and lifecycle actions.
- Keep a temporary compatibility adapter for current scenario syntax.
- Delete each old dispatch branch when its plugin reaches parity.

Exit gate: no central action switch remains; every plugin has schema, deadline,
structured evidence, and focused failure tests; current reference artifacts show
no semantic drift.

### Phase 3 — capabilities and structured evidence

- Move backend-specific decisions into capability providers/adapters.
- Replace prose-first action results with the common evidence envelope.
- Make scoring, diagnostics, redaction, humanisation, and mutation analysis
  derive from that envelope.
- Reject unclassified exceptions and unbounded attachments.

Exit gate: every check outcome has one machine-readable cause; unsupported,
inconclusive, app failure, and harness failure cannot be conflated; sensitive
data tests and diagnostic-quality tests pass.

### Phase 4 — authoring tools and calibration automation

- Scaffold packs, checks, recipes, fixture sets, actions, references, and mutations.
- Generate action/capability documentation from schemas and registry metadata.
- Produce an invalidation report from a recipe diff.
- Automate null, mutation, reference, repeated-run, and promotion checklists.
- Add focused selection by recipe/pack/check/action.

Exit gate: a new check using existing actions can be added and fully validated
without editing engine code; changing a pack or L1 recipe creates the correct
new identities and an explicit, machine-generated requalification plan.

### Phase 5 — replay and CI tiers

- Add deterministic replay for assertion, scoring, mutation classification,
  redaction, and report generation.
- CI tier 1: schema, compiler, pure plugins, replay, and unit tests.
- CI tier 2: container smoke and backend action contracts.
- CI tier 3: affected null/mutation/reference calibration.
- Promotion tier: repeated cross-backend live qualification under one frozen
  engine and recipe hash.

Exit gate: most grader changes can be debugged in seconds from retained evidence;
live runs are reserved for behavior that actually requires live infrastructure.

## Guardrails

- No host execution mode.
- No unbounded subprocess, fetch, browser action, or diagnostic capture.
- No action-selected arbitrary destructive target; all destructive operations
  derive from authenticated leases.
- No pack-provided shell in promotable recipes.
- No catch-all that converts infrastructure failure into absence, zero, or an
  application defect.
- No meaning change without a new recipe identity and comparison cohort.
- No reference promotion without null controls, exact mutation targets,
  repeated clean Docker runs, frozen hashes, and verified teardown.
- Experimental escape hatches are explicitly non-promotable until replaced by
  typed plugins.

## Design decisions still required

- Runtime schema library and whether authoring remains JSON or moves to a more
  ergonomic source format compiled to JSON.
- Plugin discovery: explicit static imports are preferred initially; dynamic
  third-party loading would expand the security and compatibility surface.
- Human versions use semantic versions while canonical content hashes supply
  exact identity. The remaining decision is the promotion and alias-moving policy.
- Capability policy for cross-backend comparisons when one backend cannot
  provide the strongest evidence channel.
- Retention and privacy policy for replay attachments.
- Whether score normalization across recipe versions is ever allowed. The
  safe default is no.

## Success measures

- Time and files changed to add a check using existing actions.
- Time and files changed to add a new action.
- Percentage of invalid recipes rejected before resource acquisition.
- Percentage of result/report paths derived from the shared evidence envelope.
- Replay coverage of non-live grader logic.
- Number of backend conditionals outside adapters.
- Qualification wall time and infrastructure-invalid rate.
- Historical artifact readability across engine and recipe upgrades.

## Estimated implementation sequence

These are engineering estimates, not campaign-runtime estimates. Some work can
overlap, but schema compilation and action extraction should precede substantial
new L3-L5 authoring so new scenarios do not deepen the legacy monolith.

1. **Specification and compatibility freeze — 1-2 engineer-days.** Name the v2
   contracts, capture normalized current behavior and golden artifacts, and
   predeclare intentional versus legacy behavior.
2. **Pack/recipe compiler layer — 4-7 days.** Runtime schemas, legacy migration,
   compiled check catalog, dependencies/conflicts, explicit inheritance,
   meaning/execution identities, validation and semantic diff.
3. **Action extraction — 5-8 days.** Move the 47 legacy steps into registered
   modules with structured evidence and direct contract tests, initially without
   behavior rewrites.
4. **Fixtures/calibration/artifact v2 — 4-7 days.** Version fixture sets, migrate
   active tracks, type null and mutation lifecycle, and add legacy readers and
   v2 writers.
5. **Agent and stack boundaries — 6-10 days.** Claude plus deterministic
   fake/fault agent adapters; three internal stack adapters; remove backend
   command switches from browser grading.
6. **Campaign scheduler and static report — 5-8 days.** Frozen experiment plans,
   counterbalanced repetitions, resource metadata, statistics, verification,
   and self-contained report generation.
7. **OCI appliance/security release — 7-10 days.** Signed images, SBOM/checksums,
   secrets flow, `preflight`, persistent artifacts, recovery behavior and runbook.
8. **Clean-runner acceptance and measured campaign — approximately 1-2 weeks
   elapsed**, depending on provider runtime and the predeclared sample size.

The expected engineering total is approximately **5-8 engineer-weeks** before
the appliance and comparison are production-defensible. A smaller internal
campaign can run sooner, but should not be presented as the final external
reproduction package.

## Future seams to preserve, not necessarily build in v1

- persisted, idempotent run state and verified resume after host/daemon failure;
- injectable clock, randomness, identifiers, latency and fault sources;
- seeded property/metamorphic invariants alongside authored examples;
- correctness, load, scale and soak profiles sharing packs but not outcome
  semantics;
- local Docker and future remote-worker implementations of one lease/artifact
  protocol;
- scheduler admission for CPU, memory, disk, browser and database slots;
- action/schema deprecation metadata and automatic pack/recipe upgrades;
- rebuildable query index over immutable artifact bundles;
- explicit statistical/repetition policy rather than averaging flakes away;
- adversarial qualification for mount escape, evidence spoofing, secret leakage,
  destructive-target selection and post-teardown persistence;
- ownership/review policy for engine, pack, recipe, fixture, and calibration changes.
