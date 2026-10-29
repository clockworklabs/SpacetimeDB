# Prompting

StackBench gives the coding agent a normal software request. It does not say
that the agent is in a benchmark. What the prompt asks for and what StackBench
measures are separate choices, and this document covers the first.

## What the agent receives

Each prompt is assembled from versioned files and bound to the campaign by
hash:

| Part | Owner |
| --- | --- |
| Framing: build a new app, or add to the existing one | Recipe |
| The features to build now | Feature packs |
| Production requirements the campaign chose to disclose | Specification packs |
| Stack setup: connection details, start command, selected SDK material | Guidance profile and backend document |
| The starting catalog | Fixture |
| Application interface: the element names and server operations the checks use | Feature contracts |
| A repair report, in repair runs only | Repair policy |

A new-build prompt looks like this, abridged:

```text
Build the application described below and leave it running.

Build the app in /app.
The web application must listen on 0.0.0.0.

## Stack
<connection details, start contract, selected SDK material>

## New application
<framing>

## <feature>
<feature request>

## Starting catalog
<fixed starting data>

## Application interface
<element names and operations>
```

The prompt never contains grader source, scenario files, check names, points,
scores, comparison results, the exact values a scenario uses, features that are
not ready yet, or production requirements under the `expected` and `observed`
treatments.

By default the prompt also contains one line asking for production quality:
"Build a production-quality application suitable for real users, not a
prototype or demo." It is on for each condition with `productionQuality: true`
and recorded with the run. It changes the request, not the checks.

## Production requirements and their treatment

A feature is product work. A production requirement says how features must
behave under conditions such as reload, reconnect, concurrent writes, access
boundaries, or direct data changes. Each selected requirement has one
treatment:

| Treatment | In the prompt | In the score | Repair feedback |
| --- | --- | --- | --- |
| `requested` | Yes | Yes | Yes |
| `expected` | No | Yes | After a conclusive failure |
| `observed` | No | No | No |

For example, a campaign selects accounts as current work and session
durability as `expected`. The agent sees only the account feature. StackBench
still checks that a signed-in session survives a reload, and a failure counts.
Switching the same requirement to `requested` adds one line to the prompt and
changes nothing else, so the two conditions are comparable.

`expected` is what lets a no-repair study measure production behavior that was
not asked for. The features, interfaces, and SDK material can still hint at a
requirement, so audit their text before claiming a behavior was unprompted.

A product request states what the product does, clearly enough that it is not
ambiguous: reviews, cancellation before shipping, reservations, restock
scheduling. It does not give implementation instructions for restart survival,
exactly-once execution, or cross-account isolation. Those are specification
packs and enter the prompt only as `requested`.

## Stack setup

Every stack's setup names the stack, its connection details, and the start
contract. The agent chooses libraries, architecture, and project structure.
Stacks that are newer and less represented in model training also get SDK
skills; today that is SpacetimeDB. A condition's `guidanceProfile` picks one
of these, from [`conditions/catalog.json`](../conditions/catalog.json):

| Profile | SpacetimeDB skills |
| --- | --- |
| `neutral` (default) | TypeScript server, TypeScript client, CLI |
| `neutral-dev` | The same, plus the `spacetime dev` watch workflow |
| `neutral-managed-dev` | The same, plus a managed start/status/stop helper |
| `neutral-no-sdk` | None |
| `neutral-dev-no-sdk` | Dev workflow only |
| `prescribed` | SDK skills plus stack documents with design advice |

Only the SpacetimeDB material differs between the neutral profiles. The skills
are study inputs: keep their text intact, and compare profiles in separate runs.

## Agent tools

Every agent has files and a shell. Two tool groups follow the campaign's
`tools` setting, the same for every agent: `webSearch` (the agent's own search
and fetch tools) and `subagents`. Both are off by default, and `subagents` is
refused for every agent, because the contamination audit cannot see what a
sub-agent does. The container can reach public websites whatever the setting.

The audit treats a fetch of StackBench's own source as contamination: anything
under `tools/stack-bench` in `clockworklabs/SpacetimeDB` on any host, GitHub or
a mirror, and its pull request. The rest of SpacetimeDB's source is open.

## Application interfaces

Feature contracts name the element IDs and server operations the checks use.
They do not prescribe layout, data model, framework, or visual design. The one
data-shaped exception is the stock interface, which other systems write to
directly. A contracted action must be the real UI action, not a separate
endpoint that can pass while the product is broken.

## Dependency progression

Each level's prompt contains the features that are ready now: a new app gets
the root features, an upgrade gets the newly unlocked ones. Upgrades keep the
original catalog and the interface contracts from earlier prompts
(`mode.retainPriorContracts`, default `true`), and never reset live data.
Blocked features are left out until their prerequisites pass.

## Repair requests

In repair runs, a repair starts only after grading records a conclusive
application failure. The agent gets a plain bug report:

```text
Fix the reported application bugs.

Expected: The signed-in account remains active after a reload.
Actual: The page returned to the signed-out state after reload.

Change only what is needed. Do not alter behavior that is already correct.
```

The report gives the expected behavior, the grader's finding in one sentence,
the measured values, and the store's own console errors. It never includes
scenario scripts, fixture data, or implementation advice. Provider failures,
harness failures, and interrupted work never become bug reports.

A repair can disclose an `expected` requirement, under the same rule for every
stack. Later levels carry earlier repairs, so a later level's first build is a
pre-repair checkpoint, not a fresh measure of unprompted behavior.
