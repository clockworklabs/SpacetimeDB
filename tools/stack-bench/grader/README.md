# Stack Bench grader

Executes versioned scenario specs against a running generated app and scores each
check from structured browser, transport, lifecycle, and database evidence.

```bash
node grade.mjs --url http://localhost:6173 --level 1 --label spacetime-l1 --out report.json
```

Each actor in a scenario gets its own browser context, so identities are genuinely
separate. The grader does not reload a failed assertion and retry it; a live-update
check therefore passes only when the already-open page updates.

## Scoring rules (enforced in code)

Each passing check contributes exactly its declared points. A failed check contributes
zero. An unavailable measurement is recorded separately and never changes the denominator.
Setup failures are copied onto every affected check, while browser console errors remain
diagnostic evidence rather than silently changing unrelated scores.

Server-side authorization and replay checks earn points only when the requested call was
actually issued and produced verifiable evidence. If the harness cannot perform the call,
the check is inconclusive and scores zero; visible interface behavior cannot substitute for
the missing server proof.

## Scenario format

Scenario files contain features with isolated `actors`, unscored `setup` steps,
and explicitly normalized criterion points. The full 51-action language is
defined in `definition-compiler.mjs` and registered in `action-catalog.mjs`;
scenario prose is not the runtime contract.

Executors are split by responsibility: ordinary browser actions, actor and
transport actions, and concurrency/lifecycle/database actions. Each executor
receives only the capabilities declared by its plugin. `grader/grade.mjs`
orchestrates actors and scoring but contains no action switch or backend-specific
database/process implementation.

`expect` supports `contains`, `notContains`, `absent`, `within` (ms), and `in` to scope
the search inside a container:

```json
{ "do": "expect", "actor": "bob", "testid": "unread-badge",
  "in": { "testid": "room-item", "contains": "{room:unread-main}" }, "within": 5000 }
```

`{room:NAME}` expands to that scenario run's unique room name.

## Writing assertions that don't lie

Two false-positive classes found while building this, both worth guarding against in new
scenarios:

- **Scope anything that repeats.** An unscoped `unread-badge` matched a *different room's*
  leftover badge and passed a feature that was entirely broken. Use `in` whenever the
  element appears once per room/message/user.
- **Assert on text, not element presence.** Apps commonly render a persistent empty
  container (e.g. `typing-indicator`) that is always visible. Presence checks on those pass
  and absence checks fail, regardless of behavior — both wrong. Use `contains`.

Use `commands/run-suite.mjs` for normal grading. It owns database reset,
provenance verification, contract linting, scenario execution, and bundle
creation. Direct `grade.mjs` invocation is useful only for focused authoring and
does not replace those run-level preconditions.

Each suite retains credential-redacted `grader-<suite>.stdout.log` and
`grader-<suite>.stderr.log` beside its report. If a grader process exits before
writing JSON, those files are the authoritative failure diagnostics.

Validate new scored scenarios with both null controls and source-bound defects:

```bash
npm run test:null
npm run check:mutations -- --app <reference-app> --mutations <manifest>
```

## Validating the grader itself

A grader that passes everything is worthless; one that fails the wrong thing is worse.
`mutation-test.mjs` injects known defects into a known-good app and checks the grader
notices at the exact declared criterion, without collateral failures:

```bash
node mutation-test.mjs --app <app-dir> --url <url> --mutations mutations/spacetime-l1.json
```

Backend, track, level and scenario come from the mutation manifest. The runner
fails closed if the baseline is not fully passing, an anchor is dead or
ambiguous, reset/redeploy/readiness fails, setup breaks, evidence is
inconclusive, the wrong criterion fails, or the intended kill has collateral.
It writes an atomic criterion-level artifact for both completed controls and
harness failures. A mutation that SURVIVES is an oracle hole, although an
equivalent mutant can also survive; confirm the edit changes observable
behaviour before diagnosing the grader.

A defect may edit one file with the manifest-level `file`, or several files by
putting `file` on each entry in `edits`. Multi-file defects are applied and
restored as one control; every file is backed up before any edit is written,
and a failed restore stops the run from reusing that source tree.

## Clean state is a precondition

Residual rows can change list, uniqueness, and aggregate assertions. The normal
runner resets the selected run-owned database before each suite, verifies the
configured database identity, and records reset or provenance failures as
harness failures rather than application scores.

## Watching a run

`--media <dir>` records one video per actor plus a full-page screenshot at the exact
moment any assertion fails. `--trace` additionally writes a Playwright trace per actor,
steppable with DOM snapshots and network:

```bash
node grade.mjs --url http://localhost:6273 --level 1 --feature 4 --label postgres --media ../media
npx playwright show-trace ../media/postgres-f4-bob.trace.zip
```

Videos are `<label>-f<feature>-<actor>.webm`, screenshots `<label>-f<feature>-<criterion>.png`.
Watching Bob's video for a failing feature is the fastest way to confirm a verdict is real
before reporting it, and the recordings double as the evidence artifact published with results.
`media/` is gitignored — recordings belong with the run output, not the repo.

## Verify the execution target

Do not infer a backend from an ID shape or a conventional port. Preflight binds
the selected stack adapter, database/module name, allocated ports, container
identity, and run lease. `run-suite.mjs` then verifies database provenance before
grading. A mismatch is a harness failure and must not produce an application
score.

When several stacks fail the same check, inspect the structured evidence and
recorded media before attributing the result to an application. Cross-stack
agreement is useful diagnostic evidence, but it is not itself proof that either
the apps or the oracle are wrong.
