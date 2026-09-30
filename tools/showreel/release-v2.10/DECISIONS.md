# v2.10 video: decisions

Range: `v2.9.0 → v2.10.0` (17 commits; 2.10.0 shipped three days after 2.9.0). 2.10.1 and 2.10.2 roll into the 2.11 video
(scheduler concurrency #5736, `spacetime.json` server for list/rename/mcp #5883, SDK perf #5840, agent setup guide
#5928/#5968, HTTP compression #5698). Length 27 s: a thin release makes a short video. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| MCP on Maincloud | Coding agents can now use SpacetimeDB's MCP tools (SQL, reducer calls) on databases hosted on Maincloud, not just local ones. | MCP · NOW ON MAINCLOUD / Connect your AI agent to your Maincloud databases. |
| Card | C# modules built for .NET 10 call reducers directly, 6–12% faster on insert/filter/scan benchmarks. | PERFORMANCE / Also in 2.10. |

## Sources
v2.10.0 release notes ("MCP support on Maincloud"), #5849, #5793; #5610.

## Wording choices
- Maincloud availability rests on the release notes (the repo has only the routing hook and egress counting). No CLI upgrade is implied; the bridge is unchanged since 2.8.1. Footer: `spacetime mcp --server maincloud`.
- C# speed: benchmark-attributed range, .NET 10 only (.NET 8, the macOS default, is flat; some microbenchmarks were flat or slower).

## Illustrative
The agent chat and the my-game tables.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): mid-session WebSocket errors now reach `onDisconnect` instead of stalling the client (#5707), C++ auto-increment symbol clashes (#5836), the 2.9.0 `spacetime dev` regression (#5867), commitlog fdatasync on open (#5830, #5829).
- jsonwebtoken upgrade, snapshot worker, CI, version bumps.
