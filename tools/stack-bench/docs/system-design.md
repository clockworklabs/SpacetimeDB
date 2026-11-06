# System design

StackBench turns one versioned plan into comparison evidence that can be traced
without chat history or operator memory. Every decision, action, result, and
cost has one owner and one durable record.

## One owner for each fact

| Layer | Owns | Durable output |
| --- | --- | --- |
| Definitions | Product work, prompt modules, checks, stacks, models, budgets | Versioned source files |
| Compiler | The exact work matrix and every bound identity | `plan.json` |
| Job store and worker | Submission, host placement, credential references, the execution claim | Job and claim records |
| Admission | Whether this plan can run on this appliance | Admission artifact |
| Scheduler | Attempt order, concurrency, continuations, terminal state | `state.json` |
| Run engine | Build, grade, repair, resource ownership, cleanup | Attempt directory |
| Grader | Typed check results and evidence | Grade artifacts |
| Progression engine | Open, passed, failed, and blocked features | `progression-state.json` |
| Report | A reproducible view of the evidence | `report.json`, `report.html` |

No layer replaces a decision from the layer above it. A view can summarize
durable data; it cannot create run state.

## Where code lives

The source is TypeScript, built to ESM JavaScript in `dist/`.

| Path | Owns |
| --- | --- |
| `src/campaigns/` | Plan compilation, scheduling, jobs, budgets, reporting |
| `src/progression/` | Feature dependencies, work selection, repair state |
| `src/composition/` | Pack and recipe compilation, prompt composition, identities |
| `tracks/` | Product work, interfaces, scenarios, feature graphs |
| `src/agents/`, `container/` | Coding adapters, provider protocols, the broker, transcripts |
| `src/stacks/`, `src/runtime/` | Stack operations, isolation, source checkpoints, cleanup |
| `grader/`, `src/evidence/` | Executable checks, seeded bugs, evidence validation |
| `commands/`, `dashboard/` | CLI and browser interfaces to the same engine |
| `skills/` | Operator procedures |

## Terms

One word per thing. The CLI, dashboard, and artifacts use these.

- **campaign**: one comparison job, defined by a
  [plan](../appliance/README.md#plans); a result directory holds everything it
  produced. The dashboard calls a campaign a run.
- **attempt**: one stack building the product once inside a campaign.
- **execution**: one process run of an attempt. A retried attempt has two.
- **session**: one conversation with the coding agent: a build session or a
  repair session.
- **stack**: the technology under test. Flags still spell it `--backend`.
- **level**: one rung of a sequential campaign. **depth**: how far down the
  feature graph a dependency campaign has reached. Same field, different meaning.
- **feature**: one node of the graph, the unit the agent builds and the grader
  scores. It opens when its parents pass.
- **questline**: a named path of features through the graph, such as
  `identity-profiles` or `orders-fulfillment`.
- **check**: one scored criterion with a stable ID such as `601b`. Feature
  checks measure product behavior; production checks measure required
  properties of it.
- **treatment**: how a specification reaches the agent; see
  [prompting](prompting.md).
- **first build**: the score before any repair. **repair**: one paid session
  that reacts to a failure report, plus the regrade after it.
- **passed**, **failed**, **blocked**, **inconclusive**, **harness failure**:
  the check outcomes, defined in [grading](grading.md). A **provider failure**,
  a harness failure, and a **contaminated** attempt are excluded from comparison.
- **qualification**: matching reference, seeded-bug, and empty-app evidence for
  an exact check selection. Without it, results are provisional.
- **preflight**: the checks before an attempt starts. **admission**: the record
  that preflight and policy allowed it.
- **coding container**: where the agent works, created from the build image.
- **clean source**: the accepted source with nothing the agent's process left
  behind. Every grade starts the store from clean source.
- **broker**: the proxy that holds the provider key so the coding container
  never sees it. Its **cost receipt** proves what a session spent.
- **lease**: the record of the containers, ports, database, and locks an
  attempt owns, so cleanup and recovery touch only those.

## Data flow

```text
versioned definitions
        |
        v
compiled plan -> admission -> scheduler -> run engine -> grader
                                      |          |          |
                                      v          v          v
                                  state.json  run.json   evidence
                                      \          |          /
                                       \         v         /
                                        -> inspection -> report
```

What the agent receives is in [prompting](prompting.md).

## Appliance isolation

The controller owns definitions, grading, results, provider secrets, and the
Docker socket. It runs on Docker's host network so it can reach the ports each
attempt publishes on loopback. The dashboard runs on a bridge and publishes only
`127.0.0.1:7331`; its controls start a fresh controller through Compose.

Each attempt owns one database container and one bridge network. The database
container anchors the network namespace: the coding container, browser, smoke
container, and broker join that namespace and share nothing with other
attempts. Before creating anything, the controller writes the attempt's lease and
claims its port and capacity locks. It records creation authority before
`docker create` and exact IDs before `docker start`. A short helper with
`NET_ADMIN` then loads the nftables rules, and only then does the database
start. Generated code and the browser cannot change the firewall.

The namespace may reach its own loopback, the package cache at its address on
port 4873, and public IPv4 HTTP and HTTPS. It cannot reach the host, private
ranges, other attempts, or IPv6. The package cache joins each attempt's bridge
for the attempt's life; publishing and sign-up are disabled on it.

The browser has no Docker socket, provider secret, app mount, or control port;
Playwright drives it through `docker exec`. The coding container gets its app
workspace, its transcript directory, and the selected stack's dependency
mounts, with a read-only root filesystem and `no-new-privileges`. It never
gets the repository, grader, scenarios, results, Docker socket, or credential
file. The broker gets the credential file; the agent gets a session token.

All state lives in the `stack-bench-state` volume, mounted in the controller
at Docker's own mountpoint so child bind mounts name the same bytes on Docker
Desktop and Linux. PostgreSQL and MongoDB get private per-attempt credentials
and unpublished ports. MongoDB runs as a single-node replica set, which allows
transactions and change streams but does not test failover.

Teardown removes the exact owned containers, volumes, and network, releases the
locks, and leaves anything it cannot prove it owns. A restart of the database
container invalidates the recorded namespace and requires recovery of the whole
attempt; see the [recover-run skill](../skills/recover-run/SKILL.md).

## Operator loop

1. **Define**: select one versioned plan. Do not rebuild a plan from flags.
2. **Validate**: compile it and inspect the attempts, model, prompt policy,
   checks, points, budgets, images, and parallelism.
3. **Admit**: prove credentials, images, ports, capacity, and stack access
   before model work starts.
4. **Run**: start the stored plan or an eligible continuation. A paid action is
   always explicit.
5. **Observe**: read durable campaign state first; open logs only to diagnose.
6. **Decide**: continue only through a legal state transition. Never hide an
   invalid attempt or retry it outside the frozen policy.
7. **Report**: generate the result from retained evidence.
8. **Clean**: remove temporary owned resources and keep the campaign package.

The CLI and the dashboard share the compiler, scheduler, state reader, and run
commands. Machine-facing commands return stable JSON that names the plan,
campaign state, active work, cost, failures, and legal next actions.
