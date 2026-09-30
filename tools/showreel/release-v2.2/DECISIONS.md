# v2.2 video: decisions

Range: `v2.1.0 → v2.2.0` (120 commits; v2.1.0 sits on an empty side commit off master). Length 37 s. Research and sources: `RESEARCH.md`.

## Not shown although the 2.2.0 release notes list them
`spacetime lock` / `unlock` (#4502) and better module panic backtraces (#577) were reverted by #4881 before the tag. Lock shipped in 2.7 and is in that video.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| Remove tables | Developers can empty a table with the new `clear()` call, remove it from the module and republish; SpacetimeDB drops the empty table instead of requiring a full wipe. | MIGRATIONS / Remove tables without wiping the database. |
| Safer CLI | `spacetime delete` now asks for confirmation, `spacetime list` shows database names, and `publish --yes=…` skips only the prompts you name. | CLI / The CLI asks before deleting a database. |
| Cards | The TypeScript SDK sends calls from the same tick in one message; React gets `useProcedure` and `useTable({ enabled })`; a new Astro template; Windows binaries are signed. | APPS AND TOOLING / Also in 2.2. |

## Sources
#4593, #4729; #4770, #4769, #4885; #4761, #4784; #4752, #4721; #4688; #4906 (signature checked on the published v2.2.0 assets).

## Wording choices
- Remove tables: "Only empty tables are dropped" is on screen. The 2.1 lane shows the old limitation (manual migration), not a bug.
- Performance: no numbers (only PR-author laptop measurements exist); the transport change is a card, not a speed claim. Batching is client → server only.
- `spacetime list`: only named rows shown (an unnamed database shows an empty name cell).
- Delete prompt shown interactively; the CI line uses `publish --yes=…`, where the new values exist.

## Illustrative
Database names, identities, row counts.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): `client_connected` not guarding HTTP SQL (#4563), primary-key changes breaking the next publish (#4666), auto-increment counters resetting after restarts (#4902), Unreal macOS build failure and duplicate `OnInsert` (#4712, #4903), JS out-of-memory handling (#4777), TypeScript module fixes, SQL negative numbers.
- Bytes-key B-tree indexes (optimization), `Timestamp` filters, C# `IEnumerable` views, config error messages, durable publish, `--native-aot` (conflicting platform claims), templates' `.gitignore`, refactors, benchmarks, CI, tests, docs.
