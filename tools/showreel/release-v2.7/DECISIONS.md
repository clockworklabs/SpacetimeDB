# v2.7 video: decisions

Range: `v2.6.0` → published 2.7.0 (`v2.7.0-hotfix3`), 100 commits, including v2.6.1. The literal `v2.7.0` tag is only the
version-bump commit; the published 2.7.0 holds most of its release notes. Length 46.5 s. Research and sources: `RESEARCH.md`.

## Script
| Scene | What shipped, for whom, what they can now do | Kicker / headline |
|---|---|---|
| MCP endpoint | Every database now exposes an MCP endpoint, so AI agents can read its schema, run SQL and call reducers with your identity. | NEW · MCP ENDPOINT / An MCP endpoint for every database. |
| Lock | `spacetime lock` protects a database: while locked, it can't be deleted or wiped until someone runs `spacetime unlock`. | NEW · SPACETIME LOCK / Lock a database against deletion. |
| Unique constraints | Developers can add `#[unique]` or `#[primary_key]` to an existing table and republish without wiping data; duplicates stop the publish with a list. | MIGRATIONS / Add unique constraints to live tables. |
| Cards | C# modules on .NET 10 compiled ahead of time; typed queries in Unreal (C++ and Blueprint); Svelte `reconnect(builder)` to sign in without a reload; `spacetime sql --format json`; TypeScript `onSchedule`. | MORE FEATURES / Also in 2.7. |

## Sources
#5489, #4888, #4465, #4915, #4810, #5375, #5459, #5435.

## Wording choices
- MCP: first MCP support; no `spacetime mcp` command yet (2.8.1), not on Maincloud yet (2.10), so the endpoint URL is local and no agent product is named.
- Lock: blocked commands show only "403 Forbidden" (the CLI's exact text wasn't verified). The lock blocks deletion and resets only; verified for standalone servers.
- Unique constraints: the 2.6 error text is the real one; `user_email_key` is illustrative. No automatic de-duplication is implied.
- .NET 10: no speed claim; Windows and Linux only.

## Illustrative
The agent chat, gold values, the identity in the lock output, user rows and emails.

## Left out
- Fixes (rule: don't advertise embarrassing fixes): empty `ctx.sender` in procedures since 2.4 (#5323), Unreal overlapping-subscription cache desync (#5426), SQL writes ignoring accessor names (#5478), `client_disconnected` not running on some HTTP calls (#5498), commitlog replay table lookup (#5513), TypeScript module fixes.
- TS camelCase handles (breaking), C++ view primary keys and C#/Unreal `Find()` on views (parity), Rust SDK traits, the CLI tagline, `init --template` listing, login/start/log fixes, metrics, docs, CI, tests.
