# v2.0 video: decisions

Range: `v1.12.0 → v2.0.1`. The `v2.0.0` tag is an unreleased feature-branch tip with no GitHub release and no CLI binary;
the public "SpacetimeDB 2.0" release is `v2.0.1`, so the video covers what users actually got. The 2.1 video therefore starts at `v2.0.1`.
Length 62.5 s. No fix cards were removed in the rework: 2.0's items are features and by-design changes (not broadcasting reducer arguments is a 2.0 protocol design change, shown as a privacy gain). Research and sources: `RESEARCH.md` (written against `v2.0.0`; items marked "v2.0.1 only" there are included here).

## Many 2.0 release-note headlines shipped in 1.x and are not shown as new
TypeScript modules (1.6), React/Vue/Svelte (≤1.12), the Unreal SDK (1.4), Postgres wire protocol (1.4), `spacetime dev` itself (1.7), views (1.8),
collaborators (1.9), procedures (1.10), typed query builders (1.11/1.12), SpacetimeAuth docs, AI rule files. Maincloud pricing, free tier, dashboards
and the Team tier can't be verified from the repo.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| Event tables | Module authors can mark a table as an event table; each inserted row reaches subscribed clients as an event and isn't kept in the table. | NEW · EVENT TABLES / Broadcast events with event tables. |
| Reducer calls | In TypeScript, calling a reducer returns a promise that resolves or rejects with the module's error; other clients no longer receive the arguments you sent. | TYPESCRIPT SDK · NEW PROTOCOL / Await your reducer calls. |
| spacetime.json | `spacetime init` writes a `spacetime.json`, so `spacetime generate`/`publish` run without flags, and `spacetime dev` now also starts your client. | CLI · SPACETIME.JSON / Configure your project once, in spacetime.json. |
| Templates | Ten new starter templates (Next.js, Nuxt, Angular, TanStack Start, Remix, browser script, Bun, Deno, Node.js, C++) plus Angular and TanStack bindings. | NEW · STARTER TEMPLATES / Ten new starter templates. |
| Security cards | Scheduled functions are private to clients, private tables stay out of generated client code, procedures can't make HTTP calls into private networks. | SECURITY / Secure by default. |
| More cards | TypeScript modules out of beta, export-based TypeScript modules, typed queries without `.build()`, confirmed reads by default. | MORE FEATURES / Also in 2.0. |

## Full scenes (sources)
| Scene | Claim on screen | Source |
|---|---|---|
| Broadcast moments, not rows. | Event tables (`event: true`): each insert reaches subscribers as `onInsert`, and no rows stay in the table | #4217, #4251 |
| Call it. Await it. | TypeScript reducer calls return a promise (rejects with `SenderError`); other users no longer receive your reducer arguments | #4213, #4271 |
| Two words. Whole app. | `spacetime.json` (written by `init`) lets `spacetime generate`/`publish` run without flags; `spacetime dev` also starts the client ("Starting client: npm run dev") | #4199, #4332, #4351 |
| Pick a framework. Go live. | Ten new templates (Next.js, Nuxt, Angular, TanStack Start, Remix, browser script, Bun, Deno, Node.js, C++), plus `spacetimedb/angular` and `spacetimedb/tanstack`; React/Vue/Svelte already existed | #4139, #4107 and template PRs |

## Cards
Secure by default: scheduled functions are private (#4179); private tables aren't code-generated unless `--include-private` (#4241); procedures refuse HTTP to private IP ranges (#4243).
Plus: TypeScript out of beta (the publish warning was removed, #4396, v2.0.1); export-based TypeScript modules and pretty `console.log` (#4220, #4285); typed queries without `.build()` in TS/Rust/C# (#4261); confirmed reads by default on v2 connections (#4390, #4419, v2.0.1).

## Wording choices
- Event tables: no "never stored" or storage-cost claim; rows still go to the commitlog.
- Awaitable calls are TypeScript-only (Rust uses `_then` callbacks), hence the kicker. The error message in the demo is illustrative; `SenderError` is the real class.
- "Your arguments stay yours": the server stopped broadcasting reducer name and arguments (#4213). Shown as another user's screen before/after, which is illustrative.
- `spacetime.json`: the file content is what `init` writes. `spacetime dev` isn't called new, only "now starts your client too". The first two terminal lines are paraphrased; "Starting client: npm run dev" is the real output.
- Templates are shown as of v2.0.1 (several were broken at the v2.0.0 tag and fixed in v2.0.1).
- Private networks card: loopback is refused too on self-hosted servers; nothing suggests localhost calls work.

## Illustrative
Event table fields, damage numbers, player windows, names, the app window.

## Left out
- Performance: the release notes' "100k TPS TypeScript / 170k Rust" isn't in the repo; the only in-repo figure (107,850 TPS) is for a Rust module under specific conditions. Left out rather than risk a wrong number.
- C++ modules: in beta and pinned to 1.12 in 2.0; a "new language" claim would mislead.
- Organizations (`--organization`, Maincloud-side), the guarded 1.x → 2.0 upgrade prompt, breaking renames (`accessor`, `ctx.sender()`, …), case conversion, install mirror, internal refactors, CI, tests, docs.
