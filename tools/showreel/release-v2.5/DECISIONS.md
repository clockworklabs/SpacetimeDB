# v2.5 video: decisions

Range: `v2.4.0 → v2.5.0` (includes v2.4.1, cut from a release branch; its PRs #5111 and #5145 are also on master inside
this range). Length 46 s. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| Procedures | Rust, C# and C++ module authors can use procedures (outgoing HTTP with `ctx.http`, their own transactions with `with_tx`) without turning on the unstable feature flag. | PROCEDURES · RUST, C# AND C++ / Procedures, out of beta. |
| SolidJS | SolidJS developers get `spacetimedb/solid` hooks that keep the UI in sync with live tables, and a starter template: `spacetime dev --template solid-ts`. | NEW · SOLIDJS SUPPORT / SolidJS support, with a starter template. |
| View primary keys | A view written as code can declare a primary key, so subscribed apps get one update instead of a delete and an insert. | VIEWS · RUST, C# AND TYPESCRIPT / Views written as code can have a primary key. |
| Cards | `spacetime call` accepts `0x…`/`c200…` identities as-is; `publish -c=always` reads the database from `spacetime.json`; bulk inserts of text-heavy rows stay fast as tables grow. | CLI AND PERFORMANCE / Also in 2.5. |

## Sources
#5164, #5052, #5111 (2.4.1, Rust/TS), #5246 (C#), #5254, #5256, #5071.

## Wording choices
- Procedures: "out of beta", never "new" (behind the flag since 1.10). TypeScript never had a flag, so it isn't named. Inbound HTTP handlers are still unstable, so "call other websites" is as far as the caption goes.
- SolidJS: a lightly tested community contribution; not called stable or battle-tested. The app's title, status, input and button match the template at v2.5.0.
- Views: shown with the Rust attribute; in TypeScript the key comes from the row type, so no TypeScript "add a key" step. Query-builder views already had keys, hence "written as code". Not C++.
- Bulk inserts: worded as an improvement with no numbers (none are public).
- The v2.5.0 notes say views are still unstable; the code says otherwise, so that line was ignored.

## Illustrative
The weather procedure, the Cargo.toml line, the SolidJS app's names, the leaderboard values, the terminal success line.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): TypeScript "No such index" after a restart (#5145).
- Event-table reshaping on republish (#5269): shipped with a restart bug fixed in the next release. Update notice once a day (#5184), template version pins (#5228): low value. Billing/metrics (#5131, #4930), internal threading, tests, CI, docs, version bumps.
