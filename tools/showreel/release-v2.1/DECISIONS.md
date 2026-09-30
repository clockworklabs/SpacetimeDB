# v2.1 video: decisions

Range: `v2.0.1 → v2.1.0` (134 commits: 2.0.2 to 2.0.5 and 2.1.0). v2.0.1 is the published "SpacetimeDB 2.0" and belongs to
the 2.0 video, so v2.0.1-only items (confirmed reads by default, TypeScript out of beta, optional database names) are not here.
Length 53 s. Research and sources: `RESEARCH.md` (written against `v2.0.0 → v2.1.0`; only items after v2.0.1 are used).

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| Rust in the browser | Rust client developers can compile the SDK to WebAssembly (`features = ["browser"]`) and connect from a web page. | RUST CLIENT SDK / Rust clients now run in the browser. |
| HTTP timeouts | Procedures' outgoing HTTP calls now wait 30 s by default and up to 3 minutes (was 0.5 s / 10 s), so slow APIs like LLMs can answer; errors now include the cause. | PROCEDURES · OUTBOUND HTTP / Longer HTTP timeouts for procedures. |
| Unreal | The Unreal SDK moved to the 2.0 protocol, so Unreal clients receive event tables (C++ modules declare them with the 4th `SPACETIMEDB_TABLE` argument). | UNREAL SDK · C++ MODULES / Event tables, now in Unreal. |
| CLI cards | A fuzzy template picker, `spacetime logs --level`, a daily update notice, and `spacetime login` replacing the current session. | ALSO IN 2.1 / A smoother CLI. |
| More cards | Agent skills (`npx skills add clockworklabs/SpacetimeDB`), republishing a database with a module in another language, bare booleans in `where()`. | MORE FEATURES / Also in 2.1. |

## Sources
#4183; #4630 (2.0.5), #4610; #4497, #4461; #4470, #4362, #4363, #4367; #4172 (first in 2.0.4); #4549; #4547.

## Wording choices
- Rust in the browser: no template or docs exist, so none are shown; browser-only API (`build().await`, `run_background_task()`); CI runs the wasm tests under Node, so nothing claims browser testing.
- Timeouts: per HTTP request, 180 s is a hard cap; the LLM URL and 12 s answer are illustrative. The old limits are shown as limits.
- Unreal: not called new (announced at 2.0); this is the SDK catching up to the 2.0 protocol. No Mac shown (macOS builds came in 2.2).
- Update notice: installs through the official installer from 2.0.4 on. Skills: the command only (the default branch's skill set has changed since).
- Switching languages: lifting a limitation; the new module must still pass normal migration rules.

## Illustrative
Player rows, URLs, timings, the blob animation.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): view subscriptions stopping for other users when one disconnects (#4607, #4648, #4646, #4639), React `isReady` stuck (#4499, #4580), TypeScript/C# SDK fixes, CLI path fixes, the host-type repair (#4619).
- Rust/C# primary keys for query-builder views, TanStack SSR prefetch, schema route, pgwire upgrade, `spacetime dev`/`spacetime.json` polish, 2.0.4 API breaks, Windows signing (#4473 produced no signed binaries until 2.2), docs, CI, benchmarks.
