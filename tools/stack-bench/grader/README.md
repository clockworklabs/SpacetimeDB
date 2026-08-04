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
