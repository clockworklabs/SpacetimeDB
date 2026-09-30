# v2.9 video: decisions

Range: `v2.8.0 → v2.9.0` (2.8.1, 2.8.2, 2.8.3, 2.9.0; linear history). Length 44.5 s. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| MCP | The new `spacetime mcp` command lets coding agents like Claude Code or Codex list your databases, read their schemas, run SQL and call reducers, using your CLI login. | NEW · SPACETIME MCP / Connect your coding agent to your database. |
| Plugins | New Claude Code and Codex plugins install SpacetimeDB's skills and its MCP server in two commands each. | NEW · AGENT PLUGINS / SpacetimeDB plugins for Claude Code and Codex. |
| Unity | The Unity SDK now works with Domain Reload turned off, so Unity developers can use Unity's faster Enter Play Mode setting. | UNITY SDK / Unity SDK supports disabled Domain Reload. |
| Cards | Rust modules can give String columns a default value; C# modules reuse buffers and ran 3–29% faster in some benchmarks. | MODULES / Also in 2.9. |

## Sources
`spacetime mcp` and the host-wide route with `list_databases` (#5582); Claude Code plugin (#5672) and Codex plugin (#5582); Unity Domain Reload support (#5554, completed by #5738); Rust `#[default("…")]` on String (#5562); C# buffer reuse (#5530).

## Wording choices
- MCP: the per-database endpoint with `get_schema`/`sql`/`call` existed in 2.8.0; what's new is the `spacetime mcp` bridge, the host-wide endpoint and `list_databases`. The panel says LOCAL SERVER because MCP reached Maincloud only in 2.10.
- Plugins: the skills existed before; the plugins are what's new.
- Unity: the speed-up comes from Unity's own setting; the video says SpacetimeDB now supports it, not that SpacetimeDB made Play Mode faster.
- C# speed: the release notes' range, attributed to "some of our benchmarks" (a couple were slower).

## Illustrative
The agent chat, databases and tables; the Play Mode lanes (no durations).

## Left out
- Fixes of embarrassing bugs (rule: don't advertise them): first-time visitors losing their identity on reconnect (#5761), the commitlog power-loss window (#5785), C# scans skipping rows (#5621), the C# HTTP timeout clamp (#5751), the iOS decompress stall (#5668), accessor-rename migrations (#5544), scheduler drift (#5735), Unity WebGL builds broken since 2.7.1 (#5792).
- .NET host handling (#5571): 2.9.0's `spacetime dev` had a macOS regression. `[module-http] enabled = false` (#5774): self-hosters only, left out by the user in the first draft.
- C++ 3+ column indexes, WebSocket idle-timeout reason, C# SDK cleanup, TS `array<u8>` copy, v1 deadlock, metrics, logs, docs, CI.
- The first cut had a "Back online. Still you." scene and vaguer headlines ("Your agent, meet your database."); feedback asked for plain headlines and no embarrassing fixes.
