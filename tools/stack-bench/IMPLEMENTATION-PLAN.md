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
- [x] Explicit 47-action vocabulary and strict field validation; current
  scenarios exercise 44 actions.
- [x] Current definitions compile deterministically into definition schema v1.
- [x] Run-like artifacts declare artifact schema v2. Unversioned, older, and
  unknown future schemas fail closed; pre-v1 bytes live in a checksummed archive.
- [x] Active track manifests declare suite inheritance explicitly. Legacy v0
  manifests receive the old name-based behavior only inside the compiler.
- [x] Unit/integration baseline: 186 tests passing through typed check evidence,
  complete stack adapters, and the versioned agent-adapter boundary.
- [x] Product composition decisions are defined: behavior packs, scored recipes,
  versioned fixtures, and independent experiment plans.
- [x] Pack, recipe, fixture, and promotion source contracts are implemented for
  validated ecommerce L1/L2, with exact compatibility parity and a smoke mix.
- [x] Ecommerce L1/L2 grades resolve the candidate alias before browser work,
  prove exact legacy execution/scoring parity, and store the resolved recipe
  identity. The legacy scenario dispatcher remains the execution adapter while
  action extraction proceeds.
- [ ] The frozen three-stack live qualification is not current. An earlier
  Spacetime pass predates executable compiler changes; a Mongo repetition was
  infrastructure-invalid; PostgreSQL has not run under the new hash.

## E0 - compatibility characterization

### SB-001: Golden definition plans - complete

Capture normalized plans for each active track/level and a representative
criterion for every action. Golden updates must distinguish intentional
semantic changes from serialization-only changes.

Acceptance:

- deterministic golden fixtures cover all 47 registered actions;
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

Implemented as seven behavior packs, two exact fixture sets, L1/L2 compatibility
recipes, one explicitly scored smoke recipe, and a separate promotion catalog
holding L1/L2 as candidates. L1 compiles to the current 48 checks/51 points; L2
compiles to the current 52 checks/74 points with the same suite and feature order.
Draft packs may declare an unmeasured budget, but qualification requires a bound.
Legacy source-point scoring is rejected outside an explicitly marked migration
recipe. The live runner does not consume these candidates yet.

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
hash mismatch. Candidate status is recorded and is not presented as promotion.

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

Implemented as a strict, hash-bound calibration source and compiler. The current
ecommerce L1 calibration remains draft and all three stacks remain candidate;
this records required work without inventing qualification. It binds the exact
recipe/fixture, canonical reference registry entries and sources, exact mutation
manifests and targets, null policy, repetition counts, stack states, evidence
slots, equivalence decisions, and alias catalog. All nine zero-point checks have
one typed policy. Source validation rejects drift, incomplete controls, unknown
mutants, unsupported stack mismatches, under-specified evidence repetitions,
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
catalog for all 47 current actions. Every registration now supplies an exact
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
implementations and rejects both gaps and duplicates across all 47 actions.

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
all 47 catalog actions to have exactly one implementation.

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

Implemented as agent adapter schema v1 with static registration for the current
Claude Code implementation, deterministic loop fixture, lifecycle fault
injector, and model-free canonical-reference deployer. The adapter owns its
entrypoint, supported modes, deadline, default model, and API-key environment
name. The engine sends one normalized request and rejects unknown adapters,
unsupported modes, malformed registrations, mismatched request/result identity,
unknown result fields, invalid usage/cost/duration values, or missing setup and
session state. Result records carry normalized usage and transcript references;
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
the same fail-closed cleanup; no same-name or same-port fallback exists.

E4 exit: a clean dedicated runner can pull by digest, run `preflight`, complete a
no-model smoke, preserve artifacts, and remove the controller without losing
results.

## E5 - campaign and reporting

### SB-501: Frozen experiment manifest

Bind an exact recipe, fixtures, and calibration to stacks, models, settings,
repetitions, counterbalanced order, budgets, retry/exclusion policy, pricing
snapshot, and analysis plan before runs.

### SB-502: Idempotent scheduler

Materialize immutable attempts, enforce resource admission, resume safely, and
record every invalid attempt rather than silently retrying it away.

### SB-503: Deterministic report

Generate JSON tables and self-contained static HTML from the artifact read model.
Report dispersion, sample size, invalid-run rate, limitations, and raw-evidence
links without making causal claims the experiment does not support.

E5 exit: deleting generated report output and regenerating it from immutable raw
artifacts produces the same report identity.

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

This sequence makes the interfaces needed by parallel lanes concrete before the
large grader extraction or new production recipe authoring begins.
