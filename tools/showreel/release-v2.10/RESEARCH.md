# v2.10 research (v2.9.0 → v2.10.0)

v2.9.0 is an ancestor of v2.10.0, and there are 17 commits in range. v2.10.0 shipped only 3 days after v2.9.0, so this is a **thin release**: one headline item and a handful of cards.

Checked OUT of range (not ancestors of v2.10.0, verified with `git merge-base --is-ancestor`):
- In v2.10.1: #5736 (concurrent scheduled runs), #5883 (`spacetime list`/`rename`/`mcp` respect the project's server), #5840 (SDK perf).
- In v2.10.2 only: #5928 (agent setup guide), #5968 (Zen of Spacetime in agent setup), #5698 (`ctx.http.fetch` compression), #5845 (TS decompression sequencing).

Keep all of these out of the 2.10 video.

## Releases in range
- v2.10.0, 2026-09-04: MCP on Maincloud (leader routing + egress tracking), C# NativeAOT monomorphized dispatch (perf), TS mid-session WebSocket errors now route to `onDisconnect`, C++ auto-increment macro collision fix, fix for `spacetime dev` wrongly requiring C# settings.

## Candidates

### Your AI agent can now work with your Maincloud databases (MCP on Maincloud) — HIGH
- What changed (exact, verified):
  - The v2.10.0 notes: "The SpacetimeDB MCP endpoint (`/v1/mcp`) is now available on Maincloud. You can connect AI agents and MCP-compatible tools directly to your Maincloud databases."
  - The open-source code change supporting this is #5849. It adds `RootRoutes { ping_get, mcp_post }` and `router_with_root_routes(...)` in `crates/client-api/src/routes/mod.rs`, so a server edition (Maincloud's cluster) can route `POST /v1/mcp` to the leader replica.
  - #5793 adds egress byte counting inside `mcp_root` (`crates/client-api/src/routes/mcp.rs`), because the database is named in the request body, not in the URL.
  - The CLI bridge is unchanged from v2.8.1 (`crates/cli/src/subcommands/mcp.rs`, identical at v2.9.0 and v2.10.0). A fresh CLI's default server is `maincloud` (`crates/cli/src/config.rs`: `default_server: maincloud.nickname`), so with a Maincloud default `spacetime mcp` (as registered by the Claude Code / Codex plugins) now reaches Maincloud. `-s/--server maincloud` also works.
  - Tools: `list_databases` (databases you own; empty if anonymous), `get_schema`, `sql`, `call`, `ping`. Auth = your `spacetime login` token.
- Before this range: MCP (stdio bridge, host-wide `/v1/mcp`, plugins) worked only against local/self-hosted servers. The v2.8.1/v2.8.2 notes say "SpacetimeDB MCP is not yet available for Maincloud."
- Headline idea: "Your AI agent, now in the cloud"
- One-sentence description: Coding agents like Claude Code and Codex can now list, inspect, query and call reducers on your databases hosted on Maincloud, with the same `spacetime mcp` setup.
- Visual idea: Reuse the 2.9 MCP look (agent chat → `spacetime mcp` → database), but the database cube lifts off into a Maincloud cloud. The agent asks "Show me today's top players on my Maincloud game", a `sql` chip lights up, and rows stream back from the cloud. Optionally a small "leader" crown on one of several cluster nodes as the request is routed there.
- Caveats (don't claim):
  - Availability on Maincloud is a hosted-service deployment. The repo only contains the routing hook and egress accounting. No CLI upgrade is required for it (the bridge is unchanged since 2.8.1), so don't say "update your CLI to get Maincloud MCP".
  - `spacetime mcp` still prints `WARNING: This command is UNSTABLE and subject to breaking changes.`
  - In 2.10.0 `spacetime mcp` uses `--server` or the CLI's default server. Making it follow the project's `spacetime.json` server is #5883, which is 2.10.1, so out of range.
  - The agent-setup guide (spacetimedb.com/agent-setup.md, #5928/#5968) is 2.10.2, so out of range.
  - MCP can't publish, build or scaffold. It only operates existing databases. `sql` writes need ownership, and private tables aren't readable.
  - Don't claim "any MCP client works out of the box". Stdio clients run `spacetime mcp`; remote HTTP MCP clients would need to send the auth header themselves (not documented in range).
  - "Egress tracking" is metering. Don't market it, and don't claim anything about billing.
- Sources: v2.10.0 notes ("MCP support on Maincloud"). #5849 (commit 2fc8f82215), #5793 (commit cfec96ec57). `git show "v2.10.0:crates/cli/src/subcommands/mcp.rs"`, `v2.10.0:crates/cli/src/config.rs` (~line 167-175), `v2.10.0:.claude-plugin/README.md` ("bridges stdio to the HTTP endpoint on whichever server your CLI is configured for").

### Faster C# modules (NativeAOT direct dispatch) — MEDIUM
- What changed (exact, verified): the C# source generator now emits, in each exported FFI entry point, a `switch` on the host-provided id that calls the concrete generated reducer / procedure / HTTP handler / view / anonymous view directly, instead of `Module.__call_reducer__(id, ...)` indexing an `IReducer` list and invoking through the interface. This gives NativeAOT-LLVM a monomorphic call path. The id-based fallback is kept, and the same path is also used for .NET 8 JIT.
- Who gets it: C# modules on .NET 10 always build with NativeAOT-LLVM (`crates/cli/src/tasks/csharp.rs`: ".NET 10: always use NativeAOT-LLVM, no flag needed"). `spacetime init` defaults to .NET 10 on Windows/Linux and .NET 8 on macOS.
- PR benchmarks (.NET 10 AOT, after review, medians vs master): insert u32/u64/str btree −11.6%, insert u32/u64/u64 unique −10.5%, filter u64 index −9.2%, iterate u32/u64/u64 −7.9%, insert u32/u64/u64 btree −7.8%, insert u32/u64/str unique −6.4%, circles/ia_loop −3% to −4%. But print_bulk +0.0% to +2.3%, large args 64 KiB +2.1%. On .NET 8 JIT it is "basically flat".
- Before this range: every call went through non-generic runtime dispatch (interface call via an indexed list).
- Headline idea: "C# modules, straight to the point"
- One-sentence description: C# modules compiled with .NET 10 now call your reducers, procedures and views directly instead of through a generic lookup, making database-heavy work up to about 10% faster in benchmarks.
- Visual idea: Before, a call bounces through a switchboard/lookup table (id → list → interface → reducer). After, a straight arrow goes from the host into the reducer. A small bar chart shows −6% to −12% on insert/filter/iterate.
- Caveats (don't claim):
  - The gains are for .NET 10 NativeAOT-LLVM builds (Windows/Linux). macOS defaults to .NET 8 JIT, where it's flat.
  - Some reducer-only microbenchmarks were flat or slightly slower.
  - The numbers are from the PR author's runs, and the PR body notes the summary text was AI-generated. Don't say "10% faster" unqualified.
  - Users must rebuild against the 2.10 C# packages.
  - No API change.
- Sources: #5610 (commit 28071acffc). v2.10.0 notes ("C# NativeAOT monomorphized dispatch"). `v2.10.0:crates/cli/src/tasks/csharp.rs` (~lines 175-236), `v2.10.0:crates/cli/src/subcommands/init.rs` (macOS .NET 8 warning ~line 1640).

### Web apps recover from mid-session connection errors (TS SDK) — MEDIUM
- What changed (exact, verified): in `crates/bindings-typescript/src/sdk/db_connection_impl.ts`, a new private `#everConnected` flag is set when `InitialConnection` arrives. After that, `ws.onerror` records the error (normalized to `Error` by a new `toError`) and closes the socket. `onclose` then emits `disconnect` with that error, so `onDisconnect(ctx, error)` receives it. Errors before the initial connection still emit `connectError`, now also normalized to an `Error`.
- Before this range: any WebSocket error, even on an established connection, fired `onConnectError` (commonly wired to login recovery), set `isActive = false` so reducer/procedure calls silently queued, and never triggered a disconnect. The client stalled with no reconnect. Reported in production on `spacetimedb@2.7.1`.
- Headline idea: "Glitches heal themselves"
- One-sentence description: When a live connection hits a network error, the TypeScript SDK now reports it as a disconnect instead of freezing, so your app's reconnect logic can bring it back.
- Visual idea: A live multiplayer web app with a network error spark. Before, the UI freezes, reducer calls pile up in a queue and a wrong "login failed" toast appears. After, a "disconnected… reconnecting" pill appears and the app resumes.
- Caveats (don't claim):
  - Automatic reconnect exists only in `ConnectionManager`, used by the React/Svelte/Solid `SpacetimeDBProvider`. A bare `DbConnection` only gets `onDisconnect` with the error and has to reconnect itself.
  - Don't say "the SDK now auto-reconnects": that's not new, and not universal.
- Sources: #5707 (commit 549489e97e), issue #5706. v2.10.0 notes. `v2.10.0:crates/bindings-typescript/src/sdk/connection_manager.ts` (reconnect backoff 1 s → 30 s max, pre-existing since before v2.8.0).

### C++: tables in separate files no longer clash — LOW
- What changed (exact, verified): the auto-increment field macros in `crates/bindings-cpp/include/spacetimedb/table_with_constraints.h` built internal symbols from `__LINE__`. Two tables in different headers with an auto-inc field on the same line number produced duplicate symbols, and the **module failed to compile**. Symbols now use the table and field names (`SPACETIMEDB_AUTOINC_SYMBOL(prefix, table_name, field_name)`, export name `__preinit__19_autoinc_register_<table>_<field>`). A compile regression test (`ok_autoinc_same_line.cpp`) was added.
- Before this range: present since the C++ bindings were added (#3544, 2026-02-06).
- Headline idea: "Split C++ schemas freely"
- One-sentence description: C++ modules can now spread tables with auto-increment fields across multiple files without random build errors.
- Caveats: it was a compile error, not runtime data corruption. It only happened when the line numbers coincided. It could be promoted to a card only if the video wants a C++ item.
- Sources: #5836 (commit f08dc302ae), issue #5772.

### `spacetime dev` works again for non-C# templates — LOW
- What changed (exact, verified): in `crates/cli/src/subcommands/dev.rs`, `spacetime dev` no longer always resolves a default .NET version and passes it to `init`/`generate`/`publish`. It passes `--dotnet-version` only if the user gave one (or `spacetime.json` has `dotnet_version`).
- Before this range: **a regression introduced in v2.9.0 by #5571.** v2.9.0's `spacetime dev` always called `init::resolve_default_dotnet_major()`, even for non-C# projects. There were two symptoms:
  - (a) On macOS, every `spacetime dev` run printed `Warning: NativeAOT-LLVM does not support macOS hosts, so this C# project will target .NET 8. .NET 8 support will be deprecated soon.`. This is the warning the release note refers to.
  - (b) On any OS, when `spacetime dev` offered to create a new project with a non-C# template (e.g. `spacetime dev --template basic-rs`), `init` bailed with `--dotnet-version is only supported for C# projects (--lang csharp)`. The release note doesn't mention this.
- Promo use: mention only ("`spacetime dev` fixes"). It fixes the previous release's regression, so don't frame it as a feature.
- Sources: #5867 (commit 3663fa1128). `git show "v2.9.0:crates/cli/src/subcommands/dev.rs"` (~line 400-420), `v2.9.0:crates/cli/src/subcommands/init.rs` (~line 555).

### Commitlog opens segments with an fdatasync — LOW
- What changed: when opening an existing commitlog segment, it is now `fdatasync`ed "to ensure that any (partially) written data left by a crashed process is visible" (`crates/commitlog/src/repo/fs.rs`). #5829 also fixes the write buffer size used when resuming a segment and adds detail to a panic message.
- Promo use: at most a "more crash-safe storage" line. Too technical for a scene, and there's no user-reported symptom.
- Sources: #5830, #5829.

## Left out (not user-visible or not promo-worthy)
- #5866 Upgrade jsonwebtoken to v11: dependency upgrade. The same accepted algorithms (ES256/RS256/HS256) are preserved and old `"exp": null` tokens are still accepted. The only visible nuance is that newly minted no-expiry tokens omit `exp`.
- #5768 Change snapshot worker to use an Option: internal API.
- #5731 Enforce rollback safety PR checks: CI/process.
- #5852 CI: test Unity WebGL build (guards the 2.9 WebGL fix; CI only).
- #5848 sccache, #5857/#5861 runner labels, #5850 release workflow guard: CI.
- #5868 version bump.

## Open doubts
- **Thin release.** Only MCP-on-Maincloud carries a full scene. The C# perf card and the TS reconnect card are solid, and C++ / `spacetime dev` / commitlog are filler. Consider a shorter video or a "plus" list rather than padding with 2.10.1/2.10.2 items.
- Maincloud MCP availability can't be verified from the repo; it rests on the v2.10.0 release notes. The date Maincloud actually started serving `/v1/mcp` may differ from the tag date (2026-09-04).
- Using MCP against a Maincloud database presumably requires `spacetime login` with the account that owns it (`list_databases` shows only owned databases). Not tested live.
- #5610 benchmark numbers are from a single author's harness runs, and the PR says the summary was AI-generated. They are directionally consistent (DB-heavy workloads faster) but unverified.
- #5867: the v2.10.0 note ("no longer incorrectly warns about C# on macOS") is accurate for the macOS warning. The code additionally shows a hard error on the `spacetime dev` → new non-C# project path on any OS. Both were read from code, not reproduced by running.
- The #5610 benchmark numbers come from the PR author's runs.
