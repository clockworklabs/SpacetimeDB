# v2.3 video: decisions

Range: `v2.2.0 → v2.3.0` (58 commits). v2.3.0-hotfix1 (no GitHub release) only reverts a 2.2.0 energy change and is not
included. Length 37.5 s. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| Godot | Godot (.NET) developers get an official SDK package and a step-by-step tutorial for building a multiplayer game. | NEW · GODOT SDK / An official SDK for Godot. |
| Pipelining | The server now takes a client's next request without waiting for the previous one, so a slow procedure no longer holds up the rest; TypeScript SDK clients also get several replies per network message. | SERVER · PIPELINING / Requests no longer wait in line. |
| Cards | Vue and TanStack get `useProcedure`; web builds made with Unity 6 can connect; `spacetime init`'s AI-assistant rules now cover C++, Unity and Unreal. | SDKS AND TOOLING / Also in 2.3. |

## Sources
#4920; #4962, #4973, #5051, #5061; #4999, #4984; #4961; #4740.

## Wording choices
- Godot: .NET build of Godot 4.6.2+, no GDScript, tutorial mechanics only (leaderboard and splitting came in 2.4), desktop only.
- Pipelining: no speed numbers (the benchmark's method changed completely between releases); the lanes are illustrative; batched replies only for TypeScript SDK clients. The 2.2 lane shows the old limitation ("each request waits its turn").
- Unity 6 WebGL: worded as Unity 6 support (a new engine version), not as a fix.
- AI rules: installing rules isn't new; the card says their coverage grew.

## Illustrative
The Godot file tree, player names, food, the request lanes and timing.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): Windows snapshots failing with "access denied" since 2.2.0 (#4939), the V8 segfault on old-ABI JS modules (#4986), false view migrations (#4985), Rust SDK connect-error handling (#4935, #4938), JWT error bodies (#5000).
- Release-note items that don't hold up: deferred compression (reverted), prepared-statement rollback (benchmark only), "compression knobs" (not compression settings), HTTP/2 (cleartext only). `listen_addr` in `cli.toml`, `database_identity`, unstable `db_read_only`, `version uninstall` error, docs, CI, benchmarks.
