# v2.6 video: decisions

Range: `v2.5.0 → v2.6.0`. v2.5.0-hotfix1 is a side branch; its only fix (#5288) is also on master inside this range.
Length 35.5 s: a thin release. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| React reconnect | React apps using `SpacetimeDBProvider` now reconnect by themselves after a dropped connection (backoff from 1 s, doubling up to 30 s) and re-subscribe, with no page reload. | REACT APPS / React apps now reconnect automatically. |
| C++ query builder | C++ module authors can write views as typed queries (filters, semijoins) that the compiler checks, as Rust, C# and TypeScript already could. | C++ MODULES / Typed queries for C++ modules. |
| Card | C# tables can use `Timestamp` as a primary key, like Rust and TypeScript ones. | C# MODULES / Also in 2.6. |

## Sources
#5185, #4664, #5262.

## Wording choices
- Reconnect: limited to apps using the React provider (not `DbConnection` built by hand, Vue, Svelte or Angular; Solid probably but untested). No identity claim. Backoff numbers are the code's; the outage and which attempt succeeds are illustrative.
- C++: code from the query-builder test module, lambda parameters shortened; the feature is undocumented at v2.6.0, so no docs URL.

## Illustrative
Chat messages, user names, the live view's rows, the build line.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): the scheduler surviving a crash in one scheduled call (#5280), `spacetime subscribe -n` exiting successfully on too few updates (#5278), the event-table restart bug from 2.5.0 (#5288, #5289).
- The release notes' headline (view primary keys) shipped in 2.4.1/2.5.0 and is in the 2.5 video; commitlog settings and the AWS download move are in v2.3.0; ARM cross-compile is CI-only; #5322 was never merged. Query-engine refactors, docs, CI, tests.
