# Prompting method

Stack Bench gives the coding agent a normal software request. It does not tell
the agent that it is in a benchmark. What Stack Bench asks for and what Stack
Bench measures are separate choices.

## Prompt inputs

Each request is assembled from these owners:

| Input | Purpose | Owner |
|---|---|---|
| Product framing | Says whether to build a new app or add work to an existing app | Recipe |
| Current features | Describes the product work to implement now | Feature packs |
| Requested production behavior | States production requirements that the campaign chose to disclose | Specification packs |
| Stack material | Gives required access details and the selected level of technical guidance | Guidance profile and backend document |
| API reference | Supplies selected SDK material, including SpacetimeDB skills | Guidance profile |
| Starting data | Gives the fixed catalog for a new app | Fixture |
| Application interface | Names the controls or operations needed for reliable use | Feature contracts |
| Repair report | Describes conclusive application failures from the last grade | Condition repair policy |

The recipe and selected packs own the text. The prompt builder orders that text
and adds the small controller contract, such as the application directory,
listening address, start script, and completion response.

## What the coding agent receives

A new-build request has this shape:

```text
Build the application described below and leave it running.

Build the app in /app.
The web application must listen on 0.0.0.0.

## Stack
<selected stack access and guidance>

## Selected API reference
<selected SDK material, when needed>

## New application
<product framing>

## <current feature>
<feature request>

## Starting catalog
<fixed starting data>

## Application interface
<stable controls or operations>
```

This is an abridged example. The exact request is composed from versioned files
and bound to the campaign by hashes.

The new-build request does not include:

- grader source or scenario files;
- check names, point values, expected scores, or comparison results;
- exact adversarial inputs chosen by a scenario;
- future dependency nodes that are not ready;
- production expectations assigned to the `expected` or `observed` treatments.

## Features and production expectations

A feature is product work. A production expectation describes how selected
features should behave under conditions such as reload, reconnect, concurrent
writes, authorization boundaries, or direct data changes.

Each selected production expectation has one treatment:

| Treatment | Included in request | Main score | Repair feedback |
|---|---:|---:|---:|
| `requested` | Yes | Yes | Yes |
| `expected` | No | Yes | Yes, after a conclusive failure |
| `observed` | No | No | No |

`Expected` answers: "Does this stack produce sound production behavior when the
user did not prescribe the mechanism?"

`Observed` is a separate first-build diagnostic. It cannot change the main
score or steer repairs.

### Example: expected durability

The campaign selects account creation as current work. It also selects session
durability as expected production behavior.

The coding agent sees product text such as:

```text
## Accounts

Visitors can create an account with a username and password. Returning users
can sign in, see which account is active, and sign out.
```

The request does not mention reload behavior. Stack Bench can still verify that
the signed-in session survives a reload. A conclusive failure affects the main
score and can produce repair feedback.

### Example: requested durability

The campaign selects the same account feature and changes durability to
`requested`. The request now also includes text such as:

```text
## State durability: accounts

A signed-in session survives a page reload as the same account.
```

The scored behavior is the same. Only disclosure changed. This makes the two
conditions comparable without changing the feature itself.

### Example: observed durability

The campaign changes durability to `observed`. The request again omits reload
behavior. Stack Bench measures it after the first build, records the result as
a diagnostic, and does not include it in the score or repair report.

## Stack guidance

Stack selection and guidance selection are separate.

- Neutral guidance gives the required stack, connection details, startup
  contract, and selected API reference. The coding agent chooses libraries,
  architecture, and project structure.
- Prescribed guidance can add design advice selected by the campaign.

SpacetimeDB can include its TypeScript SDK skills in either condition. This is
API reference for a less familiar stack. It does not expose grading logic or
the implementation that a check expects.

An abridged neutral PostgreSQL section is:

```text
# PostgreSQL

Use PostgreSQL for the application data. Choose the libraries, architecture,
and project structure.

Use the supplied DATABASE_URL. Serve the application on the supplied port.
```

An abridged neutral SpacetimeDB section is:

```text
# SpacetimeDB

Use SpacetimeDB for the application data. Put the TypeScript module in the
required module directory. Choose the schema, libraries, architecture, and the
rest of the project structure.

Use the supplied server URI, module name, CLI, SDK package, and web port.
```

## Application interfaces

Feature contracts name stable controls or operations when deterministic use
requires them. They do not prescribe layout, data models, frameworks, or visual
design. The one data-shaped item a contract may name is an interoperability
surface that other systems write to directly, such as the stock tables; the
behavior expected around that surface stays in the specification.

For example, the account contract names fields such as `signup-username` and
`signin-submit`. An HTTP stack also exposes the account operations through HTTP.
A reducer-based stack exposes the equivalent reducer operations. The product
behavior stays the same while the usable interface matches the selected stack.

Scenario files own exact test data and edge-case values. Those values do not
belong in the product request or interface contract.

## Dependency progression

Dependency mode composes the request from features that are ready now.

- A new app receives the framing, current root features, applicable requested
  production expectations, starting data, and their interfaces.
- An upgrade receives only the newly ready feature work and its interfaces.
- Passed features are not repeated unless they fail again.
- Blocked descendants are not included until their dependencies pass.

For example, if accounts and catalog items are ready, the request can include
those two features. Customer profile stays out until its account dependency
passes. A failure in the catalog path does not add or remove work from the
account path.

## Repair requests

A repair starts only after Stack Bench completes grading and records a
conclusive application failure. A repair report can name an expected
production behavior that the initial request withheld; that disclosure
happens only through the repair policy, after a conclusive failure, and under
the same rule for every stack. The coding agent receives a plain bug report:

```text
Fix the reported application bugs.

Expected: The signed-in account remains active after a reload.
Actual: The page returned to the signed-out state after reload.

Change only what is needed. Do not alter behavior that is already correct.
```

The real report also identifies the affected product area and includes the
current feature text and application interface. Provider failures, harness
failures, and interrupted work do not become application bug reports.

Every line of the report comes from one of three sources: the sentence the
request already gave the agent for that behavior, a finding from the
grader's closed catalog rendered as one sentence (a control that did not
appear, a number that read 9 instead of 12, a request that was accepted
when it had to be refused), and the application's own console errors.
The grader never writes prose into the report, and the values a scenario
chose to probe a behavior are never among a finding's fields. A repair
fixes the behavior, not the probe.

## Authoring rules

- Put product asks in feature prompts.
- Put optional production requirements in specification prompts.
- Put stable controls and operations in contracts.
- Put connection facts and API material in stack guidance.
- Put exact values and probes in scenarios.
- Never solve a check by adding its private input or expected implementation to
  the request.
- Keep equivalent stacks equally informed about the product.
- Give every replay, forgery, or direct call a named application action that
  declares both the HTTP route and the reducer. A campaign does not compile
  while a selected check cannot be measured on a selected stack.
- Mark changed prompt inputs as draft until matching qualification is current.

After a prompt change, run `npm run check:composition`, `npm run check:prompts`,
and the exact scenario check for the affected recipe. Inspect the rendered
request for every affected stack and depth. Do not run unrelated qualification
or paid work.
