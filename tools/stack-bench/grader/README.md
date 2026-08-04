# Stack Bench grader

Executes versioned scenario specs against a running generated app and scores each
feature 0–3 from observed browser behavior only. Replaces the manual two-Chrome-profile
grading pass.

```bash
node grade.mjs --url http://localhost:6173 --level 1 --label spacetime-l1 --out report.json
```

Each actor in a scenario gets its own browser context, so identities are genuinely
separate (apps key identity off `localStorage`). The grader never reloads a page except
in the refresh probe, so a feature that only works after a reload cannot pass.

## Scoring rules (enforced in code)

| Rule | Effect |
|---|---|
| Setup steps fail | feature scores 0 (untestable) |
| Assertion fails, but passes after a reload | feature capped at 1 |
| JS console errors during the feature | feature capped at 2 |

## Scenario format

`../scenarios/level-NN.json` — features, each with `actors`, unscored `setup` steps, and
`criteria` worth 1 point each. Actions: `register`, `createRoom`, `enterRoom`, `send`,
`typeInto`, `clearInput`, `click`, `wait`, `expect`.

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

Reset the app's database before grading (`../../llm-sequential-upgrade/reset-app.sh <app-dir>`);
leftover rows from earlier runs are the main source of spurious passes.

Validate new scenarios against a known-broken app before trusting them. `../linter/fixtures/mock-chat.html`
is a useful negative control: it has no backend, so it should score 1/12.

## Validating the grader itself

A grader that passes everything is worthless; one that fails the wrong thing is worse.
`mutation-test.mjs` injects known defects into a known-good app and checks the grader
notices, in the right feature:

```bash
node mutation-test.mjs --app <app-dir> --url <url> --mutations mutations/spacetime-l1.json
```

A mutation that SURVIVES (score unchanged) is a hole in the oracle. Watch for *equivalent
mutants* — a defect that changes no observable behavior. One here removed a receipt's
sender filter, but a second filter still excluded the same user, so nothing changed and
the survival was meaningless. Confirm a mutation actually alters what a user would see
before treating its survival as a grader bug.

## Clean state is a precondition, not a nicety

Grading a dirty database silently biases scores DOWNWARD: an accumulated room/user list
breaks assertions that pass on a clean app. Feature 1 scored 3/3 on a fresh database and
2/3 on a dirty one, repeatably. The grader records `environment.preexistingRooms` and
warns when it is non-zero — treat any such run as non-comparable.

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

## Verify which backend answered

Both Express backends default to port 6001, so whichever server started last owns it and
the other app's client silently proxies into it — producing confident scores for the wrong
system. This has happened twice. Before grading an Express app, confirm the API is the one
you think: `curl -s localhost:6001/api/rooms` returns integer ids for Postgres and 24-char
hex ObjectIds for MongoDB.
