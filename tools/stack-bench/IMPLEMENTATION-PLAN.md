# Stack Bench v1 implementation plan

This file turns `EXTENSIBILITY-ROADMAP.md` into ordered, reviewable work
packages. The roadmap defines the product and architecture; this plan defines
what can be implemented now, what depends on an earlier contract, and what
evidence closes each package.

## Fixed v1 decisions

- Execution is Docker-only. There is no host-mode compatibility path.
- The first supported environment is a dedicated disposable Linux x86-64
  runner. General workstation support is not a v1 claim.
- Human-authored JSON packs, recipes, and fixture manifests compile into a
  normalized execution plan. Runtime code does not consume unchecked sources.
- L1/L2 are aliases to promoted recipes, not engine branches. A recipe selects
  exact behavior-sized pack versions and fixture versions.
- Packs own builder requirement/contract fragments and pass/fail checks. Recipes
  own global framing and scoring weights. Experiment plans independently own stacks, models,
  repetitions, ordering, budgets, retries, exclusions, and analysis policy.
- Pack and recipe releases have a human version plus an exact content hash.
  They carry separate meaning and execution fingerprints. Meaning changes start
  new comparison cohorts; execution-only comparability requires an explicit,
  evidence-backed decision.
- Test IDs are permanent. Pack lifecycle is `draft`, `qualified`, or `retired`;
  retired definitions remain available for exact reruns. Pre-v1 result bytes are
  archived separately and are not accepted by the active artifact reader.
- L1/L2 alias mappings are versioned promotion records. Results store both the
  alias used and the exact resolved recipe hash.
- Action, stack, and agent adapters use explicit static registration in v1.
  Loading arbitrary third-party code is outside the v1 trust boundary.
- Artifacts are immutable and additive. Invalid attempts never replace original
  evidence; pre-v1 bytes are archived without semantic migration.
- The initial live agent may be provider-specific behind the adapter contract;
  campaign and artifact schemas may not be provider-specific.
- Final qualification and measured campaigns run only after executable inputs
  are frozen. Any executable harness change invalidates that qualification hash.

## Dependency map

```text
E0 compatibility characterization
        |
        v
E1 packs + recipes + identity
        |
        +--------------------+
        v                    v
E2 action runtime       E4 appliance shell
        |                    |
        v                    |
E3 evidence + adapters <-----+
        |
        v
E5 campaign + reports
        |
        v
E6 clean-runner qualification
        |
        v
E7 measured comparison + handoff
```

E4 can proceed alongside action extraction, but it must not freeze a public
manifest or result contract ahead of E1 and E3. L3-L5 can be explored while
these packages land, but cannot be promoted or used in a measured campaign.

## Current baseline

- [x] Docker-only build sessions, exact resource leases, bounded operations,
  fail-closed reset/teardown, and structured top-level run outcomes.
- [x] Explicit 49-action vocabulary and strict field validation; current
  scenarios exercise 44 actions.
- [x] Current definitions compile deterministically into definition schema v1.
- [x] Run-like artifacts declare artifact schema v2. Unversioned, older, and
  unknown future schemas fail closed; pre-v1 bytes live in a checksummed archive.
- [x] Active track manifests declare suite inheritance explicitly. Legacy v0
  manifests receive the old name-based behavior only inside the compiler.
- [x] Unit/integration baseline: 291 tests passing through typed check evidence,
  complete stack adapters, and the versioned agent-adapter boundary.
- [x] Product composition decisions are defined: behavior packs, scored recipes,
  versioned fixtures, and independent experiment plans.
- [x] Pack, recipe, fixture, and promotion source contracts are implemented for
  validated ecommerce L1/L2, with exact compatibility parity and a smoke mix.
- [x] Ecommerce L1/L2 grades resolve the alias before browser work,
  prove exact legacy execution/scoring parity, and store the resolved recipe
  identity. All scenario actions execute through versioned, capability-scoped
  registries.
- [x] Ecommerce L1 has current repeated Docker reference, mutation, and null
  evidence for all three stacks and is promoted.
- [x] Ecommerce L2 has current repeated Linux/amd64 Docker reference, mutation,
  and null evidence for all three stacks and is promoted.

## E0 - compatibility characterization

### SB-001: Golden definition plans - complete

Capture normalized plans for each active track/level and a representative
criterion for every action. Golden updates must distinguish intentional
semantic changes from serialization-only changes.

Acceptance:

- deterministic golden fixtures cover all 49 registered actions;
- changing action fields, points, suite membership, or inheritance produces a
  focused diff;
- source ordering that has no semantics does not change the normalized identity.

Implemented as canonical, human-diffable plans for ecommerce, chat, and the
internal loop track plus a synthetic compatibility definition representing all
47 registered actions. `npm run check:definition-goldens` is read-only; accepting
intentional semantic drift requires the explicit `--update` command.

### SB-002: Golden artifact read model

Retain representative legacy-v0 and schema-v1 run, grade, bundle, mutation, null,
and qualification artifacts. Define the stable in-memory read model used by
reporting and migration tests.

Acceptance:

- every artifact kind either loads into the read model or fails with its exact
  unsupported schema/kind;
- readers never rewrite source evidence;
- golden failure classification and score remain unchanged.

E0 exit: current semantics are captured well enough to detect drift during the
grader extraction. This is a test freeze, not a claim that every legacy behavior
is desirable.

## E1 - packs, recipes, fixtures, and identity contracts

### SB-101: Source-version and inheritance boundary - complete

Schema-v1 track manifests must declare `inherit` for every suite. The compiler
alone normalizes legacy-v0 suite-name behavior. Runtime suite selection consumes
only the explicit policy.

### SB-102: Pack, recipe, and fixture source contracts - complete

Define versioned source schemas and migrate the current ecommerce benchmark into
approximately five to eight behavior-sized packs. Define recipes that select
exact pack versions, fixture sets, task instructions, and scoring weights. Define
fixture manifests for starting data and controlled external dependencies. Keep
L1/L2 as aliases to promoted recipes during migration.

Acceptance:

- packs group coherent behavior rather than test technique and carry permanent
  pack/check IDs, lifecycle, requirements, capabilities, dependencies,
  conflicts, actions, evidence needs, and budgets;
- packs do not own study-specific weights; recipes do;
- fixture identity includes users, products, stock, permissions, controlled time,
  and external stubs where used;
- a smoke recipe demonstrates intentional reuse without duplicating checks;
- missing versions, duplicate check IDs, cycles, conflicts, missing fixtures,
  unsupported capabilities, and ambiguous scoring fail before resource acquisition;
- the compatibility recipe preserves current L1/L2 execution and scoring under
  the existing goldens.

Implemented as eight behavior packs, two exact fixture sets, L1/L2 compatibility
recipes, one explicitly scored smoke recipe, and a separate promotion catalog
holding promoted L1 and L2 aliases. L1 compiles to 48 checks/51 points; L2
compiles to 53 checks/75 aggregate points with the same suite order.
Draft packs may declare an unmeasured budget, but qualification requires a bound.
Legacy source-point scoring is rejected outside an explicitly marked migration
recipe. The live runner consumes the exact resolved release.

### SB-103: Compiled recipe release and identity - complete

Compile a recipe into one canonical plan containing resolved packs, stable check
keys, normalized actions, prompt/contract bytes, fixture references, weights,
and capability requirements. Give it a human release id, exact content hash,
meaning fingerprint, and execution fingerprint using a documented serializer.

Acceptance:

- two compilations are byte-identical;
- object-key or source-file ordering with no semantics does not change identity;
- changing requirements/assertions/scoring changes the meaning fingerprint;
- changing selectors/timing/actions/fixtures changes the execution fingerprint;
- every exact change alters the overall content hash and requires requalification;
- every grade and bundle stores the recipe identity and compiled check catalog
  needed to interpret it later.

Implemented with canonical meaning, execution, and combined content hashes.
Human versions and lifecycle state remain visible but do not substitute for the
hashes. The public release contains exact component identities, task/source
digests, a source manifest, and a compact stable-key check catalog; fixture
credentials and the full executable plan are not copied into results. L2 binds
the exact L1 release hashes. Ecommerce L1/L2 runner compatibility fails before
browser work on alias ambiguity, level/scoring/plan drift, or a parent/child
hash mismatch. Promotion status is recorded explicitly and never inferred from
a passing score.

### SB-104: Calibration manifest - complete

Create a calibration manifest that binds an exact recipe hash to fixture hashes,
canonical references, null expectations, exact mutation targets, repetition
policy, supported stacks, equivalence decisions, and promotion state.

Acceptance:

- missing or stale fixture/mutation hashes fail preflight;
- zero-point controls have typed roles and explicit promotion policy;
- individually qualified packs do not make a recipe publishable; the exact
  combination must qualify as a whole;
- an execution-only comparability decision names both hashes and its supporting
  calibration evidence;
- an alias cannot move to an unqualified recipe, and resolving an alias records
  the promotion-catalog version;
- calibration cannot be reused after recipe or fixture drift without an explicit
  new manifest.

Implemented as a strict, hash-bound calibration source and compiler. Ecommerce
L1 and L2 are qualified from their exact repeated evidence. L2 binds measured
pack budgets, two reference and two mutation repetitions for each supported
stack, one null control, and a required Linux/amd64 appliance runner. A
calibration binds the exact recipe/fixture,
canonical reference registry entries and sources, exact mutation manifests and
targets, null policy, repetition counts, stack states, evidence slots,
equivalence decisions, and alias catalog. Every zero-point check requires one
typed policy. When declared, runner identity participates in qualification
identity and is required on reference, mutation, and null evidence. Source
validation rejects drift, incomplete controls, unknown mutants, unsupported
stack mismatches, under-specified evidence repetitions,
and draft calibration presented as promoted. `check:calibration` is the direct
validation entry point and is part of Linux CI; the appliance's public
`preflight` command will call the same compiler in SB-401.

### SB-105: Complete artifact identity envelope - complete

Add artifact kind, engine identity, pack/recipe/fixture/calibration/experiment
identities, agent-adapter identity, stack-adapter identity, parent attempt, and
timestamps to one common envelope. Add kind-specific payload schemas beneath it.

Acceptance:

- every producer uses the common writer and every consumer uses the versioned
  reader;
- partial/invalid attempts remain representable without fake score fields;
- unknown kinds or versions fail closed;
- secrets and lease tokens do not enter the public envelope.

Implemented as artifact schema v2 with strict envelope and per-kind payload
fields, atomic writes, engine/recipe/pack/fixture/calibration/experiment/agent-
adapter/stack-adapter identity slots, attempt ancestry, and start/completion
timestamps. Current public run, grade, bundle, lint, action, mutation, null,
reference, performance, bug-quality, and public lease-evidence producers use the
common writer; reporting and qualification consumers use the versioned reader.
Legacy v0/v1 evidence remains read-only, while unknown versions, explicit
unknown kinds, unknown v2 fields, malformed payload shapes, and secret-bearing
keys fail closed. The deterministic loop verifies the real run-to-child chain;
Docker image and injected-restart cleanup tests verify the container boundary.

### SB-106: Pack and recipe authoring commands and runtime selection - complete

Implement `pack validate`, `recipe validate`, `recipe show`, and `recipe diff`,
followed by focused selection by pack/check for intentionally scoped runs.

Acceptance:

- commands are read-only and require no Docker daemon;
- `diff` separates meaning, scoring, fixtures, execution, and metadata changes;
- output includes the exact calibration work invalidated by a change.

The read-only command surface and its acceptance checks are implemented.
`recipe show --pack/--check` and `bench.mjs --pack/--check` use the same pure
resolver. Pack and check requests form a union; the executor grades only the
resolved stable check keys. Run, bundle, and grade artifacts retain requested,
resolved, attempted, and not-run scope, and comparison fails closed across
different recipe or selection identities. Subsets are valid run scopes;
qualification must cover that exact scope.

### SB-107: Pack-owned requirement composition - complete

Make optional product capabilities genuinely composable by moving their public
builder requirements and contract fragments into the same versioned packs that
own their checks. Recipes retain only global task framing and select exact pack
versions. A check filter may narrow measurement, but it must never silently
change the task the application was built to satisfy.

Acceptance:

- selecting a pack adds its requirement/contract fragments and its checks;
- removing a pack removes both, unless another selected pack explicitly owns
  the same shared requirement;
- pack requirements compile in a deterministic documented order and contribute
  to the recipe meaning and content hashes;
- dependencies and conflicts apply to task composition as well as grading;
- `recipe show` displays the exact composed builder task, and `recipe diff`
  identifies requirements added or removed by pack;
- public run evidence records the exact composed-task identity independently of
  any later check-only measurement filter;
- the ecommerce compatibility recipe splits account/session durability into a
  coherent independently selectable capability without changing the legacy
  full-recipe prompt or score.

Implemented with ordered, source-contained requirement and hook fragments owned
by packs, plus recipe-owned global framing. Fragment markers must exist exactly
once, shared fragment IDs must resolve to identical bytes and metadata, and
dependencies/conflicts are resolved before either task or check composition.
The release records separate requirement, contract, and composed-task hashes;
`recipe show` includes the exact task text even when a check-only filter narrows
measurement, while `recipe diff` names added, removed, and changed fragments.

Ecommerce L1 now has a separate `session-durability` pack with criterion-level
ownership. Removing it removes the session requirement and three checks without
removing ordinary account access. Full L1 and L2 reconstruct their previous
prompt and appendix bytes exactly and retain 48/51 and 52/74 parity. The agent's
production prompt path consumes the compiled task rather than independently
reading the whole level documents. The draft L1 calibration was rebound to the
new exact recipe identity; no qualification or comparison claim was carried
forward.

The production runner now enforces that statement as well as the authoring
compiler: `--pack` composes the actual agent prompt from selected packs plus
their transitive dependencies, while `--check` only narrows measurement within
that requested scope. The controller passes a compact content-identified task
request to the agent, which recompiles and verifies it before rendering a prompt
or calling a model. Campaign conditions bind explicit packs, resolved task
packs, exact requirement/contract fragment IDs, and their hashes. The Claude
adapter contract is version 1.8.0 for this request; it declares the long-lived
subscription-token environment separately from API-key billing and rotating
interactive credentials.

E1 exit: L1 resolves to a deterministic recipe release composed from exact pack
and fixture versions, and the builder task is composed from the same selected
packs rather than independently maintained whole prompts. A second smoke recipe
proves reuse and mix-and-match validation.

## E2 - registered action runtime

### SB-201: Action plugin contract - complete

Define the registered action interface: versioned input schema, capability
requirements, deadline, executor, structured evidence schema, redaction tags,
and renderer metadata. Define a narrow run context rather than exposing the
grader's mutable state.

Acceptance:

- duplicate/unknown registrations fail at startup;
- every action declares a deadline and evidence type;
- a deterministic fake context contract-tests success, application failure,
  inconclusive evidence, timeout, cancellation, and unclassified exception.

Implemented as a strict schema-v1 plugin/registry contract plus a compatibility
catalog for all 49 current actions. Every registration now supplies an exact
input compiler, semantic capability requirements, a hard deadline, typed
evidence validation, redaction tags, renderer metadata, and an executor that
receives only its declared capabilities and exact implementation function. The
real grader and scenario checker construct this registry at startup. Duplicate,
missing, unknown, malformed, unbounded, cancelled, timed-out, non-serializable,
and unclassified paths are covered by fail-closed tests. The legacy dispatch
branches remain only as implementations to migrate incrementally in SB-202 to
SB-204; this ticket does not claim that extraction is complete.

### SB-202: Pure and observation actions - complete

Extract timing, input, navigation, and browser-observation actions in small
groups. Delete each corresponding legacy branch only after golden parity.

Implemented as 16 independent registered executors: wait; click, fill,
press-key, reload, typing and input clearing; and the nine browser observations.
They receive actor access, token expansion, recorded-number state, timers, and
test-id rendering through narrow declared capabilities. Application assertion
mismatches become typed failed evidence; unexpected executor faults remain
harness failures. The migrated branches and duplicate helpers were deleted from
the grader, including the refresh probe's bypass of registered dispatch. Static
coverage proves every action is implemented either by this registry or exactly
once in the remaining compatibility dispatcher. Golden plans did not drift;
153 unit/integration tests, the live Playwright grade, loop, fault injection,
and Docker smoke all pass.

### SB-203: Actor and transport actions - complete

Extract registration, sign-in, room/cart/message, replay, and named-call actions.
Application concepts must be supplied through narrow capabilities rather than
hard-coded backend branches inside browser grading.

Implemented as 19 independent executors covering account/session setup, room
and message operations, scheduling, wire delivery checks, request forgery and
credential-swapped replay, named concurrent calls and their outcome assertion,
and app-supplied scripts. Executors receive only actor lookup plus the exact
browser, transport-observation, named-action, application-file, or subprocess
capabilities declared by their plugins; none receives the grader context.
Backend-specific named-call target/body resolution is isolated behind a
capability provider. The migrated grader branches, replay/auth helpers, and an
unreachable duplicate restart branch are deleted. Static coverage now counts
implementations and rejects both gaps and duplicates across all 49 actions.

The extraction also closed two result-integrity holes found by parity testing:
implicit one-point criteria are materialized before scoring (preventing NaN in
memory and `null` JSON totals), and app scripts cannot escape the supplied app
root. Input contracts now expose every option these migrated executors consume.
Accepted replays are no longer recorded as server refusals before failing.
Unit tests cover typed browser failures, structural/unverified transport
evidence, credentials, named-call state, and missing capabilities. A real
Playwright mini-app proves registered signup plus observation scores 1/1.

### SB-204: Concurrency and lifecycle actions - complete

Extract barriers, concurrent dispatch, offline/reconnect, app restart, backend
restart, and direct database setup last because they have the largest resource
and classification surface.

Implemented as the final 12 registered executors: click and captured-request
barriers, nested races, concurrent send populations, offline state, close/open/
fresh clients, backend restart, app-tier stop/start, and direct stock writes.
Nested actions recurse through registered dispatch and preserve typed failure
classification. Backend/application/database behavior is supplied by narrow
capability providers; browser grading no longer contains stack-specific SQL,
process-control branches, or an action switch. Startup and static tests require
all 49 catalog actions to have exactly one implementation.

Hardening includes cleanup ownership before fresh-client navigation, browser-
crash preservation during barriers, abort propagation into lifecycle readiness
waits and named fetches, lifecycle deadlines longer than their bounded worst-
case command sequence, app-script deadlines shorter than their outer action,
SQL literal escaping, and verification that direct stock writes actually update
one row (including a read-back for SpacetimeDB). The grader shrank from roughly
91 KB to 42 KB while retaining actor capture, annotation, orchestration, and
scoring responsibilities.

E2 exit (achieved 2026-08-12): the central action switch is deleted, all 47 actions are registered and
contract-tested, and current golden plans/evidence show no unapproved drift.

## E3 - structured evidence and adapters

### SB-301: Evidence envelope - complete

Replace prose protocols with typed passed/failed/inconclusive/harness-failure
evidence, observation and expected values, phase, actor, timing, retryability,
attachments, and sensitivity metadata.

Implemented as check evidence schema v1. New grades preserve the complete
versioned action evidence trail for feature setup and each criterion; setup
blocked criteria reference the feature setup evidence instead of duplicating
it. Artifact validation rejects malformed evidence and obsolete boolean/detail
projections. Scoring, comparison, null analysis, mutation analysis,
outcome classification, qualification, and repair-report selection consume the
typed verdict. Pre-v1 artifacts are checksummed in an inert archive and rejected
by the active reader. Wording-independence tests prove summaries cannot change
classification.

### SB-302: Stack capability adapters - complete

Move SpacetimeDB, PostgreSQL, and MongoDB commands and observability differences
behind versioned capability providers. Unsupported evidence is explicit and can
never become a pass by absence.

Acceptance:

- the runner and grader select a statically registered adapter by manifest id;
- lifecycle, reset, direct database access, readiness, diagnostics, resource
  requirements, and observability are declared capabilities rather than core
  backend branches;
- each current stack passes the same adapter contract and compatibility tests;
- a deterministic minimal fake stack can be added by registering one adapter
  and fixtures, with no edits to runner, grader, scoring, or reporting code;
- missing and unsupported capabilities emit typed evidence and never pass by
  omission.

Implemented as a strict engine-facing contract plus statically registered
SpacetimeDB, PostgreSQL, MongoDB, and deterministic offline adapters. Adapters
now own ports, resource leases, reset/reseed policy, database preparation and
direct writes, lifecycle control, diagnostics, grading context, named actions,
agent setup metadata, build-container mounts/staging, reference deployment,
reporting conventions, and teardown. Required capabilities and operations are
validated at registry construction; adapter/capability identity mismatch,
partial implementations, unsupported operations, and unknown stacks fail
before acquisition. The runner, grader, reference qualification, reset,
reporting, and teardown paths contain no concrete-backend dispatch. A complete
fake adapter registers without engine edits, and isolation tests prove that
PostgreSQL/MongoDB containers do not receive SpacetimeDB SDK or credential
mounts.

### SB-303: Agent adapter contract - complete

Normalize build/upgrade/repair requests, cancellation, completion, duration,
usage, transcript references, cache settings, and raw provider metadata. Ship a
deterministic fake/fault adapter before qualifying another live provider.

Implemented as agent adapter schema v2 with static registration for the current
Claude Code implementation, deterministic loop fixture, lifecycle fault
injector, and model-free canonical-reference deployer. The adapter owns its
entrypoint, supported modes, deadline, default model, and API-key environment
name. It also declares required build-image executables, a no-model credential
status command, and whether stack skill documents are part of its prompt. The
engine sends one normalized request and rejects unknown adapters,
unsupported modes, malformed registrations, mismatched request/result identity,
unknown result fields, invalid usage/cost/duration values, missing build-image
executables, or missing setup and session state. Result records carry normalized
usage and transcript references;
the adapter identity hashes both its execution file and behavior-affecting
configuration. `--agent` arbitrary-path execution was removed in favor of the
static `--agent-adapter` boundary. The full deterministic loop exercises build
and repair; fault injection proves cleanup after a non-zero adapter exit.

### SB-304: Shared classification and rendering - complete

Make scoring, console summaries, mutation analysis, redaction, bug reports, and
HTML/JSON output consume the same evidence. Remove string-prefix and regex-based
meaning from classification paths.

Implemented with one immutable disposition table for the four evidence states.
It is now the only source of pass, measured, application-failure, repairable,
run-outcome, and operator-label semantics. Scoring, run aggregation, mutation
and null controls, comparisons, reference qualification, nested actions, repair
selection, and console output consume that table. Humanisation and redaction
remain presentation-only and cannot change a verdict. Focused adversarial tests
use summaries that deliberately claim the opposite status, including through
the real repair-report CLI.

E3 exit: each check has one machine-readable cause and backend/provider
specifics do not leak into campaign semantics.

## E4 - appliance shell and operator safety

### SB-401: `preflight` - complete

Preflight architecture, Docker/runtime access, resource floors, disk, ports,
registry digests, credentials, outbound destinations, persistent result volume,
clock, and unsupported ambient state. Provide a no-model smoke mode.

Implemented as an exact-scope command and mandatory admission gate for every
non-fixture benchmark. It checks track/levels/packs/checks, registered adapters,
host and Docker architecture, Node, Docker/Compose, CPU/memory/disk floors,
clock skew, build-image identity/platform, digest-pinned healthy database
services, free ports, provider-declared credentials and outbound destinations,
the repository Linux CLI architecture, inherited ownership state, and local
Docker topology. The model-free smoke starts the exact build image, tests
outbound TLS/DNS, and proves a container-written result persists on the host.
Paid runs always execute it and retain `preflight.json` as a typed child
artifact.

### SB-402: OCI/Compose release

Build pinned controller, build-sandbox, and service images plus Compose manifests,
SBOMs, signatures, checksums, secrets template, and persistent artifact storage.

In progress. `APPLIANCE-DESIGN.md` selects the supported v1 topology: a
dedicated disposable Linux x86-64 runner, host-networked controller, local
Docker socket, fixed persistent state path, release dependency volume, and
Compose secrets. Release-manifest schema v2 now separates honest unsigned
candidates from qualified releases. Both bind exact Linux/amd64 platform
manifest references, digest-bearing SPDX 2.3 SBOMs, checksummed
operator/dependency/Compose/secrets/support files, HTTPS-only destinations, and
safe secret targets. Candidates cannot claim a signing key. Qualified
verification requires an external matching trust key, verifies a detached
Cosign bundle over the exact on-disk manifest, and verifies every registry image
signature. Bundle verification also detects missing, changed, non-regular,
escaping, and wrong-image SBOM files.

The runtime packaging slice is implemented. The digest-pinned controller base
contains Playwright, Docker CLI/Compose, and the harness; a one-shot initializer
copies 287 exact SDK/CLI/standalone files to a marked named volume and refuses
unmarked, changed, or wrong-release content. Stack build plans accept explicit
bind or named-volume mounts and expose only stack-declared dependencies to the
coding container. The dedicated-runner Compose file uses Linux/amd64, host
networking, the local Docker socket, `/var/lib/stack-bench`, a Compose secret,
read-only controller filesystem, dropped capabilities, and digest-pinned
database services. `appliance/README.md` documents the concrete build,
preflight, run, result-retention, and risk boundaries.

The controller image builds successfully, launches Chromium, reaches Docker
29.6.2 through the mounted socket, initializes and re-verifies the dependency
volume idempotently, and passes **203/203 tests inside Linux**, including two
real Playwright grades. That Linux run exposed and fixed checkout-byte reference
hash drift, a Windows-only test path, namespace-blind port checking, and
mutation validation that happened after ambient preflight.

The supply-chain commands now generate registry-resolved SPDX SBOMs, reject a
tool result that does not bind the exact requested digest, materialize file
hashes from a strict release specification, and verify qualified releases with
Cosign without a candidate fallback. Live Docker Scout runs proved the
PostgreSQL and MongoDB SBOM path; they also caught that the original Compose
pins named multi-architecture indexes, so the appliance now pins the actual
Linux/amd64 child manifests.

This is still not a distributable release. First-party image publication, real
key-controlled signing, candidate assembly, and two clean dedicated-runner
reproductions remain. Cosign is now checksum-pinned in the controller and a real
ephemeral blob-signing round trip passes, but no release registry/key authority
was supplied, so no image signature or qualification claim was fabricated.

### SB-403: Interruption and recovery

Persist attempt state, clean only exact leased resources, and emit deterministic
quarantine/recovery instructions when cleanup cannot be proven.

Implemented. Appliance runs persist a private versioned supervisor handoff under
`controller-home` and a public typed `recovery.json` beside run artifacts.
Normal exact-owned cleanup removes the private handoff. A container/listener/
lock identity refusal keeps it, marks the attempt quarantined, names the still
claimed resources and lock keys, and supplies deterministic recovery steps.
The controller `recover` command re-authenticates that private lease and retries
the same fail-closed cleanup; no same-name or same-port fallback exists. Startup
records the launched process before readiness polling, so recovery can stop that
exact process tree. A created lease is released only while its port is empty; an
ambiguous startup or unexpected listener remains quarantined.

E4 exit: a clean dedicated runner can pull by digest, run `preflight`, complete a
no-model smoke, preserve artifacts, and remove the controller without losing
results.

## E5 - campaign and reporting

### SB-501: Frozen experiment manifest

Bind an exact recipe, fixtures, and calibration to stacks, models, settings,
repetitions, counterbalanced order, budgets, retry/exclusion policy, pricing
snapshot, and analysis plan before runs.

Implemented. A strict campaign schema and compiler now bind all of those inputs,
resolve current recipe/fixture/calibration and adapter identities, and expand a
seeded balanced rotation into immutable attempts without starting a model. Draft
plans may expose incomplete qualification honestly. Frozen plans fail unless
every selected recipe, fixture, calibration and alias is qualified/promoted and
the exact controller/build image digests, timeout, cost cap,
retry/exclusion rules, pricing, and analysis policy are present. The compiled
plan is revalidated from those inputs before scheduler resume. Execution now
refuses mismatched controller, build-sandbox, or platform identities instead of
treating frozen runtime fields as descriptive labels. If a distribution release
manifest is selected, its exact hash and image bindings are enforced as well;
internal measurement does not require unfinished signing/SBOM packaging.
Ecommerce L1 and L2 are qualified and promoted. The first measured L2 campaign
exposed a cross-criterion state dependency in the operational-view checks. L2
1.1 isolates that prerequisite and has now passed fresh final-engine
qualification: each backend's reference app passed twice, every declared mutant
was caught twice, and the empty-app null control failed all scored and unscored
checks without oracle gaps. The campaign manifest implementation itself is
complete.

### SB-502: Idempotent scheduler

Materialize immutable attempts, enforce resource admission, resume safely, and
record every invalid attempt rather than silently retrying it away.

Implemented. Typed campaign plan/state artifacts now materialize the exact
attempt schedule, allow only one active execution, append retry evidence, bind
derived output directories, and fail closed on malformed identity, schedule,
timestamps, transitions, summaries, missing artifacts, or nonzero process exit.
The execution slice now provides token-bound exclusive ownership, plan-derived
bounded launches, exact run-artifact binding, append-only retries, frozen image
enforcement, and explicit supervisor-proven interruption reconciliation.
Admission writes a typed campaign artifact covering every selected
stack for each distinct agent adapter, requires the no-model container smoke,
binds each execution to that exact successful artifact, and records the runtime
identity checked before admission. The scheduler and admission path passed in the
locked Linux appliance; runtime-manifest enforcement is rechecked whenever its
controller image changes.

Campaign admission also fails closed on correction accounting. A passing level
must say correction was unnecessary or successful and must have a perfect final
score. A failing completed level is admissible only after every declared repair
round was used and the result says the budget was exhausted. Reports count cost
to correct only when correction succeeds; spend on unresolved attempts remains
visible as correction spend instead of being mislabeled as success.

Campaign admission now also requires complete measurement. Every selected
check must produce pass-or-fail evidence on the immutable first build and the
final build, and both measured denominators must equal the exact points declared
by the campaign plan. Any inconclusive selected check makes the execution
invalid measurement: its evidence is retained for harness diagnosis, it may use
the one declared clean retry, and it contributes no comparison metrics. A real
application failure after the correction budget remains admissible data because
it is conclusive; missing measurement is not converted into an application
failure or a smaller denominator.

### SB-503: Deterministic report

Generate JSON tables and self-contained static HTML from the artifact read model.
Report dispersion, sample size, invalid-run rate, limitations, and raw-evidence
links without making causal claims the experiment does not support.

Implemented. The typed read model independently revalidates every completed run
against its planned attempt, reports every attempt/execution, keeps invalid data
separate, aggregates only present metrics using the declared dispersion, and
writes content-identified JSON plus escaped static HTML with relative evidence
links. Focused tests delete and regenerate both files byte-for-byte. Full host
and Linux appliance acceptance remain before closing the work package.

E5 exit: deleting generated report output and regenerating it from immutable raw
artifacts produces the same report identity.

### SB-504: Versioned study-condition contract

Replace the single guidance choice with a strict, content-identified condition
that independently binds the requested recipe, backend guidance profile,
per-specification grading treatment, and repair policy. Compilation resolves
the exact prompt bytes, requested material, expected requirements, observed-only
checks, repair eligibility, and content hashes into campaign and result identities.

Introduce one versioned capability definition with requested and unmentioned
observation contracts where necessary. Every selected specification has one
closed treatment: requested (prompt, score, repair), expected (score and repair
without initial disclosure), or observed (separate first-build diagnostic).
Omission means excluded. This removes the coupling between prompt disclosure,
scoring, and repair without duplicating the semantic definition across packs.

Reject missing or mutable profiles, unsupported stack/profile combinations,
overlapping treatments, unobservable expected/observed specifications,
contradictory repair policy, and any attempt to treat observed-only evidence as
scored evidence. New
campaigns require the new schema; archived pre-v1 results remain inert rather
than entering through a compatibility reader.

Implemented for the draft modular L1 path. Campaign conditions compose
versioned guidance, per-specification treatments, and repair policy. The compiler
content-identifies those inputs
and each selected backend document, binds each selected recipe's meaning,
execution, resolved task packs, exact composed requirement/contract identities,
and exact scored/observed check selection, carries the resolved condition through
attempts, run artifacts, admission, grouping, and reports, and makes the coding
agent verify the exact document bytes before a model call. Expected and observed
specification identities are removed from the agent-visible request while the
controller retains their exact selection. Expected checks remain in scored
grading and repair evidence; observed checks remain source-bound and separate.
Shared harness wording
is covered by reviewed prompt snapshots and every run records its actual prompt
hash, but incorporating the common prompt-template identity directly into the
pre-run condition contract and the shared capability definition remain before
the modular recipe can be qualified and promoted.

### SB-505: Symmetric neutral backend guidance

Create qualified neutral guidance profiles for SpacetimeDB, MongoDB, and
PostgreSQL. They may disclose only the access facts required inside the isolated
runner: database/module coordinates, ports, credentials, available SDK or CLI,
and explicitly selected ordinary API material. They must not prescribe a web
framework, ORM, live-update mechanism, persistence strategy, transaction or
locking design, or project layout.

Prompt snapshots and build-container inspection prove the boundary. A missing
neutral profile fails compilation; it never falls back to prescribed guidance.
Adding a fourth backend's neutral profile requires adapter/catalog data and
qualification, not an engine branch.

In progress. The guidance profile now owns and content-identifies stack-specific
API/skill material as well as the backend document; agent configurations no
longer hide stack guidance. Campaigns pass an explicit skill list, including an
explicit empty list, so adapter defaults cannot silently alter a study arm.
Draft neutral documents and a symmetric profile exist for all three stacks.
Compiler tests exercise prescribed and neutral as an independent campaign axis,
and boundary tests reject named framework, ORM, transport, polling, transaction,
locking, or layout prescriptions. A model-free command now renders the real
build, upgrade, and fix prompts for L1/L2 under both guidance profiles, without
requiring Docker or touching an app, and verifies all 18 exact prompt hashes
against a reviewed Linux-appliance snapshot. Actual runs already retain their
own rendered prompt hash in session provenance. Snapshot review also exposed a
shared L1 testing-interface fragment that still names Express; because that text
appears in both study arms, neutral remains draft until the interface is made
framework-neutral through a new versioned recipe rather than by mutating the
qualified L1 recipe in place. Build-container inspection and live neutral
qualification also remain.

### SB-506: Observed-only first-build measurements

Run selected observed-only checks against the immutable first-build source before
any repair. Write a separate typed artifact bound to that source hash. These
observations have no recipe points, cannot change pass/fail, do not consume a
retry or correction round, and never enter a bug report or repair prompt.

Every unmentioned observation must use the core task's public surface, a
stack-neutral named action, or grader-owned database/runtime truth. Reject checks
that rely on a withheld selector, test hook, or implementation detail. Qualify
each unmentioned check with
reference, null, exact-mutation, leak-isolation, and source-binding tests on
every supported stack.

Implemented foundation: treatment is owned directly by the selected
specification, so there is no redundant probe-profile axis. The reference adapter
can seed an empty campaign app
from the exact active fixture in the content-validated registry and rejects any
different pre-existing source. `campaign.product-brief-reference.json` fixes the
primary expected-quality condition: six requested product features, neutral
backend guidance, and five unmentioned but scored/repairable specifications.
It runs two model-free reference repetitions on all three stacks (six attempts)
with an exact 44-point denominator. External data synchronization remains
excluded because its current oracle requires a disclosed table contract. This
prepares reproducible reference input; it does not replace required live
reference, null, and exact-mutation evidence.

### SB-507: Condition-aware campaigns and reports

Allow an experiment plan to counterbalance prescribed, neutral, and defaults
conditions while binding their complete identities. Campaign admission prevents
accidental cohort merging when any recipe, guidance, treatment, repair, fixture,
adapter, model, or environment identity differs.

Reports render the exact requested/expected/observed treatment table, scored
correctness, untouched observed-only behavior, and correction outcomes. Expected
checks are part of first-build/final score and repair cost; observed-only results
have a separate denominator and cannot be averaged into score or labeled as a
successful correction.

### SB-508: Finite post-run repair grants

Allow an operator to add a bounded number of repair rounds after a conclusive
run exhausts its original budget. This is a continuation, not a campaign retry:
it starts from the exact accepted source for one level, uses the same stack,
model, condition, selection, repair policy, images, and generated repair report,
and does not pay for another full initial build. A narrowly scoped setup session
may be needed to install dependencies and start an arbitrary generated project;
its time, usage, and cost are recorded separately from correction rounds.

The original run and its declared budget remain immutable. Each grant creates a
new child artifact with its own requested rounds, cost/time limits, sessions,
grades, rollback decisions, source hashes, and outcome. The cumulative path to
correctness may be reported separately as "correct after 7 total rounds" while
the original result remains "failed after the planned 3 rounds." A standardized
grant using only ordinary scored findings is distinct from an operator-written
prompt; custom instructions create an explicitly labeled investigation branch
and never enter the primary comparison.

Eligibility fails closed. The parent must have conclusive application failures,
an exhausted repair budget, complete selected-check measurement, a verified
level checkpoint, and matching executable/runtime identities. A continuation
first re-grades the saved source on fresh backend state. If the score,
selection, denominator, or evidence availability cannot be reproduced, it stops
as a harness failure before a correction round. The setup session must leave
the checkpoint source byte-for-byte unchanged, keeping setup operational rather
than corrective. Inconclusive or interrupted attempts use campaign retry/recovery
instead; they are not repairable model failures.

Implementation stages:

1. **Checkpoint foundation - complete.** Every completed level now writes a
   source-only `level-l<N>-source/` tree plus a strict, parent-linked
   `source_checkpoint` artifact. The hash is checked against the live accepted
   source, and dependencies, build output, prompts, sessions, bug reports, and
   grading evidence are excluded. The offline orchestration loop passes, and
   the full 378-test suite passes on both the host and the exact Linux/amd64
   controller image.
2. **Continuation engine - complete.** `repair grant <run-dir> --level <N> --rounds <N>`
   applies explicit per-grant cost and wall-time caps, rehydrates
   a clean work area, verifies and re-grades the checkpoint, reuses the existing
   repair/rollback loop, and writes an append-only `repair_continuation`, bounded
   process evidence, retained logs, and a new verified checkpoint.
3. **Status and reporting - command foundation complete.** `repair status`
   reports eligibility without starting work. Continuations record planned and
   cumulative rounds, score, spend, elapsed time, baseline reproduction, setup,
   failure reason, source identity, and downstream levels that require a fresh
   run. The static campaign report and future live dashboard still need a
   continuation-chain view showing remaining typed failures and per-round
   movement.
4. **Operator actions.** Expose bounded re-grade, selected-test, diagnostic,
   cancellation, and custom-prompt branch commands through the same controller.
   Every action records its parent checkpoint, exact input, actor/time, output,
   and source effect.

## E6 - qualification and release

1. Freeze source, recipe, fixtures, calibration, experiment, adapter, image, and
   dependency hashes.
2. On a quiet dedicated runner, run repeated pristine, null, exact-mutation, and
   teardown gates for all three stacks under those same hashes.
3. Build the signed release bundle.
4. Repeat a small qualification and report regeneration on a second clean runner.
5. Publish the supported environment, limitations, recovery runbook, and support
   policy with the bundle.

Any executable change after step 1 returns to step 1. Infrastructure-invalid
attempts remain in the evidence set but do not count as stack failures or clean
qualification repetitions.

## E7 - measured comparison

Run the predeclared campaign without changing its recipe, fixtures, calibration,
agent settings, scoring, or exclusion rules. Corrections require a new experiment
identity. Deliver raw attempts, checksum manifest, report-producing code, static
report, and reproduction instructions together.

## E8 - operator dashboard

Build the web interface only after SB-508's command and artifact contracts are
stable. The browser is a view and control surface over the controller; it does
not contain grading, scheduling, Docker, or repair logic of its own. A first
release shows plans, active and completed attempts, score/coverage, remaining
failures, round-by-round source movement, usage/cost/time, logs, checkpoints,
and cleanup state. It can submit only the same bounded actions available from
the command line: start, cancel, grant repairs, re-grade, run selected tests or
diagnostics, and create a clearly labeled custom-prompt branch.

Use an append-only operation/event feed so a CLI process and the web view read
the same truth. Keep Docker access in the existing controller rather than the
web process, allow only one leased mutation per checkpoint, redact credentials
and private transcripts, and begin as a local-only appliance UI. Authentication,
remote multi-user coordination, and hosted operation are later capabilities,
not prerequisites for the local v1.

## Parallel-safe ownership lanes

- Composition lane: SB-001 and SB-101 through SB-106.
- Runtime lane: SB-201 through SB-204, after SB-201 freezes the action contract.
- Appliance lane: SB-401 and recovery prototypes can start early; published
  manifests wait for SB-105 and SB-303.
- Reporting lane: golden readers can start with SB-002; final rendering waits for
  SB-105 and SB-301.

Only one integration owner edits the legacy `grade.mjs` dispatcher at a time.
Schema/envelope files require coordinated review because every lane consumes
them. Live qualification does not run concurrently with executable harness edits.

## Definition of done for every work package

- Positive, negative, timeout, and cleanup tests appropriate to the change.
- No unbounded subprocess, fetch, browser action, or diagnostic attachment.
- No silent fallback from missing/malformed/inconclusive evidence to success.
- Changed entrypoints pass syntax checks and the complete harness suite.
- Docker contract/smoke tests run when lifecycle or images change.
- Documentation states compatibility and migration impact.
- A notable architecture, qualification, or failure-learning entry is appended
  to `JOURNAL.local.md`.
- No final qualification claim is made after the executable hash changes.

## Immediate implementation order

1. [x] SB-001 golden normalized compatibility plans.
2. [x] SB-102 pack, recipe, and fixture contracts plus ecommerce migration.
3. [x] SB-103 compiled recipe identity and stable check catalog.
4. [x] SB-104 calibration binding for the exact compiled recipe and fixtures.
5. [x] SB-105 common artifact identity envelope.
6. [x] SB-106 authoring commands and hash-bound runtime selection.
7. [x] SB-201 action plugin/evidence contract.
8. [x] SB-202 pure and browser-observation action extraction.
9. [x] SB-107 pack-owned builder-requirement composition.
10. [x] SB-203 actor and transport action extraction.
11. [x] SB-204 concurrency and lifecycle action extraction.
12. [x] SB-301 structured evidence envelope through downstream classification.
13. [x] SB-302 versioned stack capability adapters and no-core-edit fake-stack proof.
14. [x] SB-303 versioned agent adapter contract and deterministic fake/fault proof.
15. [x] SB-304 shared classification and rendering cleanup.
16. [x] SB-401 exact-scope preflight and mandatory no-model paid-run admission.
17. [ ] SB-402 distributable OCI/Compose release (runtime packaging and strict
    candidate/qualified supply-chain tooling proven; image publication, real
    signing, and two clean-runner reproductions remain).
18. [x] SB-403 interruption quarantine and authenticated recovery.
19. [x] SB-501 frozen campaign manifest (strict compiler, deterministic expansion,
    qualified L1/L2 resolution, scheduler handoff, and runtime identity enforcement).
20. [x] SB-502 idempotent scheduler (strict durable state, exclusive execution,
    campaign-wide admission, bounded launch, resume validation, and proven-cleanup
    reconciliation accepted on the host and the exact Linux appliance image).
21. [x] SB-503 deterministic report (typed JSON/HTML generation accepted on
    the host and the exact Linux appliance image with byte-stable regeneration).
22. [x] SB-504 versioned study-condition contract.
23. [x] SB-505 symmetric neutral backend guidance (implemented and prompt-snapshotted;
    qualification remains part of the next campaign gate).
24. [ ] SB-506 first-build capability probes (single-execution, source binding,
    zero-score isolation, repair exclusion, per-specification observed treatment,
    exact-fixture reference seeding, a draft three-stack product-brief quality campaign,
    one-stack Docker execution, and exact controller-image tests are complete;
    live cross-stack reference/null/mutation qualification and a frozen appliance
    condition remain).
25. [x] SB-507 condition-aware campaigns and reports (exact modular condition compilation,
    preflight, prompt/grading handoff, requested/expected/observed scope recording,
    expected scoring/repair, separate source-bound observed execution, and explicit
    treatment/report sections are complete).
26. [x] SB-508 finite post-run repair grants (checkpoint verification, bounded
    continuation execution, immutable linked artifacts, cumulative accounting,
    controller commands, offline end-to-end coverage, and host/exact-image
    acceptance are complete; the richer continuation-chain view belongs to E8).
27. [ ] E8 local operator dashboard, after SB-508 freezes the command and event
    contracts; no harness behavior is implemented in the web layer.

This sequence makes the interfaces needed by parallel lanes concrete before the
large grader extraction or new production recipe authoring begins.
