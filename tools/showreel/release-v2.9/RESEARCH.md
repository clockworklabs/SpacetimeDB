# v2.9 research (v2.8.0 → v2.9.0)

History is linear: v2.8.0 → v2.8.1 → v2.8.2 → v2.8.3 → v2.9.0 (`git merge-base --is-ancestor` checked).
There are 86 commits in v2.8.0..v2.10.0, and 69 of them are in v2.8.0..v2.9.0.
Every PR below was checked against its diff and against the code at the tags.

## Releases in range
- v2.8.1, 2026-08-12: `spacetime mcp` stdio bridge, host-wide `/v1/mcp` and the Codex plugin. Also Rust string defaults, the Unity Domain Reload reset (first version), and fixes: v1 subscription deadlock, C# rows of exactly 1024 bytes dropped, TS `array<u8>` aliasing, TS decompress stall, column-default bugs.
- v2.8.2, 2026-08-18: Claude Code plugin, fix for accessor-rename migrations, C# HTTP timeout clamp fix, C# runtime buffer reuse (perf).
- v2.8.3, 2026-08-25: fix for scheduled-function drift.
- v2.9.0, 2026-09-01: `[module-http] enabled = false` operator switch, .NET host/version handling, commitlog durability fix, TS identity kept across auto-reconnect, WebSocket idle-timeout handling, Unity generic static reset + WebGL build fix, C# SDK cleanup after errors, C++ 3+ column index queries.

## Candidates

### AI coding agents can operate your database (`spacetime mcp` + host-wide MCP) — HIGH
- What changed (exact, verified):
  - New CLI subcommand `spacetime mcp [database] [-s/--server <server>] [--anonymous]` (`crates/cli/src/subcommands/mcp.rs`, added in #5582 / v2.8.1, and byte-identical at v2.9.0 and v2.10.0). It reads JSON-RPC lines on stdin, POSTs each one to the host's MCP HTTP route, and writes the responses to stdout. On start it prints `WARNING: This command is UNSTABLE and subject to breaking changes.` and then `Serving MCP over stdio, bridging to <url>`.
  - With no database it targets the new host-wide route `POST /v1/mcp` (`mcp_root` in `crates/client-api/src/routes/mcp.rs`, routed in `routes/mod.rs`). Every tool then takes a `database` argument (name or identity). A new tool, `list_databases`, lists the databases the caller owns; it is empty for an anonymous identity. The tool list is `["list_databases", "ping", "get_schema", "sql", "call"]` (test at v2.9.0).
  - With a database argument (or the `SPACETIMEDB_DB_NAME` env var) it targets the per-database route `POST /v1/database/<db>/mcp`, and the tools drop the `database` argument.
  - Auth is the CLI's saved `spacetime login` token (`auth_header_from_saved_token`), so there is no per-agent identity setup.
  - New agent skill `skills/mcp/SKILL.md`, shipped in the plugins.
- Before this range: v2.8.0 already had the per-database MCP route `POST /v1/database/:name_or_identity/mcp` with tools `ping`, `get_schema`, `sql`, `call`. v2.8.0 had no CLI bridge, no `/v1/mcp`, no `list_databases` and no `mcp` skill (10 skills at v2.8.0, 11 at v2.9.0).
- Headline idea: "Your AI agent, inside your database"
- One-sentence description: One command, `spacetime mcp`, lets coding agents like Claude Code, Codex or Cursor list your databases, read their schemas, run SQL and call reducers, using the login you already have.
- Visual idea: An agent chat panel on the left and a terminal running `spacetime mcp` in the middle, with glowing JSON-RPC lines flowing to a database cube. The agent types "List my databases", a card with 3 DB names pops out, then "SELECT * FROM message" and rows stream back into the chat. Tool chips light up one by one: `list_databases`, `get_schema`, `sql`, `call`.
- Caveats (don't claim):
  - **MCP did NOT work on Maincloud in this range.** Both the v2.8.1 and v2.8.2 notes say "SpacetimeDB MCP is not yet available for Maincloud". Maincloud support is announced only in v2.10.0. A fresh CLI's default server is `maincloud` (`crates/cli/src/config.rs`), so a 2.9-era demo must show a local server (`spacetime start`, `spacetime mcp -s local`) or a self-hosted one.
  - The command is marked UNSTABLE. The codex README also says it "may not be in your released CLI yet" (stale; it ships since v2.8.1).
  - Don't claim MCP itself is new: the per-database endpoint and `get_schema`/`sql`/`call` existed in 2.8.0. What's new is the one-command stdio bridge, the host-wide endpoint and `list_databases`.
  - The agent can't scaffold, build, publish or generate through MCP (the `mcp` skill says so). It only operates an existing database.
  - `sql` writes require owning the database, and private tables aren't readable.
- Sources: #5582 (v2.8.1 notes: "The new `spacetime mcp` CLI subcommand launches an MCP (Model Context Protocol) server over stdio"). `git show "v2.9.0:crates/cli/src/subcommands/mcp.rs"`, `v2.9.0:crates/client-api/src/routes/mcp.rs` (lines ~164-223), `v2.8.0:crates/client-api/src/routes/mcp.rs` (tools ping/get_schema/sql/call only), `v2.9.0:skills/mcp/SKILL.md`.

### SpacetimeDB plugins for Claude Code and Codex — HIGH
- What changed (exact, verified):
  - Codex plugin (#5582, v2.8.1). Install commands per `codex-plugin/README.md` at v2.9.0 (identical to the release notes):
    ```
    codex plugin marketplace add clockworklabs/SpacetimeDB --sparse .agents --sparse codex-plugin
    codex plugin add spacetimedb@spacetimedb-plugins
    ```
  - Claude Code plugin (#5672, v2.8.2). Install per `.claude-plugin/README.md`:
    ```
    claude plugin marketplace add clockworklabs/SpacetimeDB
    claude plugin install spacetimedb@spacetimedb-plugins
    ```
  - Both bundle the 11 skills (`cli`, `concepts`, `mcp`, `rust-server`, `csharp-server`, `typescript-server`, `cpp-server`, `typescript-client`, `csharp-client`, `unity`, `unreal`) and register the MCP server `{"command": "spacetime", "args": ["mcp"]}` (`.claude-plugin/marketplace.json`, `codex-plugin/plugins/spacetimedb/.mcp.json`).
  - The server skills (rust/csharp/typescript) also got extra guidance in #5583 (v2.8.2).
- Before this range: the `skills/` directory existed with 10 skills (all but `mcp`), with no plugin packaging, marketplace or install command.
- Headline idea: "Teach your AI SpacetimeDB"
- One-sentence description: Two commands install a SpacetimeDB plugin into Claude Code or Codex that teaches the agent SpacetimeDB's rules in Rust, C#, TypeScript, C++, Unity and Unreal, and connects it to your live database.
- Visual idea: Two terminal cards (Claude Code, Codex) type their two install lines. Skill badges (Rust, C#, TS, C++, Unity, Unreal) fly into an agent "brain" icon and an MCP plug snaps in.
- Caveats (don't claim):
  - Pairs naturally with the MCP scene; the two could be merged into one scene.
  - The skills themselves are not new. Only the plugin packaging, the `mcp` skill and the one-step install are.
  - Plugins install from the GitHub repo (the marketplace points at `clockworklabs/SpacetimeDB`), not from a versioned release artifact.
  - Don't say "official Anthropic/OpenAI integration". These are SpacetimeDB-published plugins in SpacetimeDB's own marketplace.
  - The MCP part has the same Maincloud caveat as above.
  - Don't show product logos in a way that implies endorsement.
- Sources: #5582, #5672, #5583. `v2.9.0:.claude-plugin/README.md`, `v2.9.0:.claude-plugin/marketplace.json`, `v2.9.0:codex-plugin/README.md`, `v2.9.0:.agents/plugins/marketplace.json`.

### First-time visitors keep their identity after a reconnect (TS SDK) — MEDIUM
- What changed (exact, verified): `ConnectionManager.#buildManagedConnection` now re-applies the token it already holds (`managed.state.token`) to the builder before every automatic rebuild (reconnect timer, resume, liveness rebuild). `rebuild()` opts out with `resumeSession: false` so that deliberate identity changes still work. The file is `crates/bindings-typescript/src/sdk/connection_manager.ts`.
- Before this range: a first-time visitor's builder was created with an empty token. After a network glitch or a backgrounded tab, the auto-reconnect went out anonymously and the server issued a brand-new identity. Rows keyed on the old `ctx.sender` stayed in the database but the client could no longer reach them. A page reload "fixed" it, so the bug looked like flakiness. Only first-time visitors were affected; returning users already had a stored token.
- Headline idea: "Reconnect without losing yourself"
- One-sentence description: If a first-time visitor's connection drops and auto-reconnects, the TypeScript SDK now reconnects them as the same user, so their data is still theirs.
- Visual idea: A user avatar with a name badge, the Wi-Fi icon flickers off/on, and the avatar reconnects with the same badge. A ghost "before" avatar gets a new random badge and its rows grey out, crossed out as "2.8".
- Caveats (don't claim):
  - Automatic reconnect lives only in `ConnectionManager`, used by the React, Svelte and Solid `SpacetimeDBProvider`s. A bare `DbConnection` doesn't auto-reconnect, so the fix applies to those providers.
  - Data was never deleted, only unreachable. Don't say "data loss".
  - Applies only to first-time/anonymous visitors whose token wasn't stored yet.
- Sources: #5761 (v2.9.0 notes: "reuses the identity token issued during a first-time anonymous connection when automatically reconnecting"). Commit c0c19366ad. ConnectionManager usage checked at v2.8.0 and v2.10.0 (react/solid/svelte providers only).

### Unity: Domain Reload disabled support + WebGL builds fixed — MEDIUM
- What changed (exact, verified):
  - #5554 (v2.8.1) added `ResetStaticFields` methods (run by Unity through `[RuntimeInitializeOnLoadMethod]`) to `Log`, `SpacetimeDBNetworkManager`, `RemoteTableHandleBase` and `DbConnectionBase`. Static state (singleton, logger, cached serializers) then resets when entering Play Mode with Domain Reloading disabled.
  - #5738 (v2.9.0) fixed that first version. Unity rejects the attribute on generic types (`Method 'SpacetimeDB.RemoteTableHandleBase`2.ResetStaticFields' is in a generic type, but [RuntimeInitializeOnLoadMethod] methods cannot be in generic types`), so the reset for generic table handles now goes through a non-generic path, and the `DbConnectionBase` reset was removed.
  - #5792 (v2.9.0) restored `using System.Collections;` in `sdks/csharp/src/SpacetimeDBClient.cs`. Unity **WebGL player builds** had failed with `error CS0305: Using the generic type 'IEnumerator<T>' requires 1 type arguments` (issue #5759).
- Before this range: Domain Reload disabled was not supported (stale singletons etc. across Play sessions). WebGL player builds were broken since v2.7.1: #5500 (commit f9f1e19e98) is in v2.7.1 and v2.8.0 but not in v2.7.0-hotfix3.
- Headline idea: "Unity: fast Play Mode ready, WebGL fixed"
- One-sentence description: The Unity SDK now works with Unity's "Enter Play Mode" fast option (Domain Reload disabled), and WebGL player builds compile again.
- Visual idea: The Unity Play button pressed repeatedly with a speed-lines effect and a clean state each time (counter resets to 0), then a WebGL browser window showing the game running, with a red build-error banner fading away.
- Caveats (don't claim):
  - Present #5554 + #5738 as one feature. The first version (v2.8.1–v2.8.3) produced that Unity error for generic types and only fully works in v2.9.0.
  - The WebGL fix repairs a regression from v2.7.1. Don't call it "new WebGL support".
  - Editor Play Mode on WebGL target was never broken, only player builds.
  - The Play Mode speed-up comes from Unity's own "Enter Play Mode Options" setting. SpacetimeDB just stops misbehaving under it, so don't imply SpacetimeDB made Play Mode faster.
- Sources: #5554, #5738, #5792, #5500, issue #5759. v2.8.1 and v2.9.0 notes. `sdks/csharp/src/SpacetimeDBClient.cs`, `sdks/csharp/src/Table.cs`.

### Rust: default values for string columns — MEDIUM
- What changed (exact, verified): Rust tables accept `#[default("…")]` on `String` columns (a `&'static str` literal). The macro change is in `crates/bindings-macro/src/table.rs`. The docs example at v2.9.0 is `#[default("")] bio: String,`, and the "Rust Limitation" note saying you **cannot** use `String` defaults was removed from `docs/docs/00200-core-concepts/00300-tables/00250-default-values.md`.
- Related (#5618, same release): new cross-language smoketests found and fixed default-value bugs. Rust negative defaults failed to build (#5622), Rust `u64` defaults errored at publish (#5623), Rust `f32` defaults were not respected (#5624), and C# `float` defaults failed to build (#5627).
- Before this range: Rust could not declare string defaults (TS `.default('')` and C# `[Default("")]` already could).
- Headline idea: "Add text columns, keep your data"
- One-sentence description: Rust modules can now give new text columns a default value, so you can add them to an existing table during an automatic migration and existing rows get that value.
- Visual idea: A table grows a new `bio` column that fills down with a default value for every existing row, with the code line `#[default("")]` on the side.
- Caveats: the value must be a string literal. Per the docs, "New columns with default values must be added at the **end** of the table definition. Adding columns in the middle of a table is not supported." Don't imply arbitrary schema edits.
- Sources: #5562, #5618 (+ issues #5622/#5623/#5624/#5627). v2.8.1 notes. `git diff v2.8.0 v2.9.0 -- docs/docs/00200-core-concepts/00300-tables/00250-default-values.md`.

### Commitlog: acknowledged writes survive power loss at segment rotation — MEDIUM
- What changed (exact, verified): after creating a new segment (temp file + rename) and after compressing a segment, the commitlog now fsyncs the new file's contents and the containing directory.
- Before this range: a power loss right after segment rotation (default `max_segment_size` 1 GiB) could leave the renamed segment unreferenced. Transactions already acknowledged as durable (confirmed reads) were then silently missing after restart. Compression could similarly replace a durable segment with an unsynced copy.
- Headline idea: "Durable means durable"
- One-sentence description: SpacetimeDB closed a narrow window where a power cut right after the log rolled over could lose writes it had already confirmed as saved.
- Visual idea: Log segments sliding along a conveyor, a lightning bolt/power cut at the seam between segments. Before, the last block vanishes; after, it locks in with a padlock.
- Before this range (verified): `v2.8.0:crates/commitlog/src/repo/fs.rs` has `tmp.as_file_mut().sync_all()?; let segment = tmp.persist(path)?;` (line ~283) and `dst.persist(...)` (line ~333), with no directory fsync. Long-standing.
- Caveats: needs an OS crash/power loss at a narrow moment (a process crash alone is not enough). Don't suggest data loss was common.
- Sources: #5785 (PR title "Acknowledged transactions can be lost when the commitlog rotates or compresses a segment"). v2.9.0 notes.

### C#: rows of exactly 1024 bytes no longer silently skipped — MEDIUM
- What changed (exact, verified): in the C# module runtime's table iterator (`crates/bindings-csharp/Runtime/Internal/ITable.cs`, `RawTableIterBase`), `Errno.EXHAUSTED` now correctly means "zero or more bytes written and the iterator is done". The removed code zeroed `buffer_len` when the final batch exactly filled the buffer (initial buffer size 1024), which dropped that batch.
- Before this range: introduced by #3909 (commit 8a0cd87c4f, 2025-12-19) and present in v1.11.2 through v2.8.0. For about 8 months, a C# module iterating or scanning a table could silently miss rows when the final batch was exactly the buffer size (e.g. a single 1024-byte row).
- Headline idea: "Every row, every time"
- One-sentence description: A C# module bug that could silently skip rows when scanning a table has been fixed.
- Visual idea: A row exactly the width of a 1024-byte "window" sliding through a scanner. Before, it falls through a gap; after, it's caught.
- Caveats: C# modules only (not the client SDK). It needed a specific size coincidence, so don't claim it was widespread.
- Sources: #5621. v2.8.1 notes. `git show a2611b0b21`.

### C#: modules run faster (buffer reuse) — MEDIUM
- What changed (exact, verified): C# module FFI exports reuse runtime buffers when consuming `BytesSource` values instead of allocating a new byte array per call (`crates/bindings-csharp/Runtime/Internal/FFI.cs`, `Module.cs`, `ITable.cs`, `IIndex.cs`, ...).
- PR benchmarks (two runs, .NET 10 NativeAOT-LLVM): iterate u32/u64/u64 −28.9%/−16.5%, iterate u32/u64/str −15.4%/−5.2%, large args 64 KiB −15.9%/−9.9%, circles/ia_loop −3% to −7%. But filter u64 index was +17.3%/+0.9%, filter string index +1.0%/+6.3%, and btree insert −4.2%/+0.0% (slower or noisy).
- Before this range: each FFI export allocated a fresh byte array for every `BytesSource` it consumed (reducer args, row iteration buffers, etc.).
- Headline idea: "Leaner C# modules"
- Visual idea: A stream of reducer calls. Before, each one spawns a new memory block that piles up as litter; after, a single buffer block is recycled in a loop. A small bar chart shows the iterate/large-args wins.
- One-sentence description: C# modules now reuse memory instead of allocating fresh buffers on every call, making common workloads a few percent to about 29% faster in benchmarks.
- Caveats: don't say "up to 29% faster" without "in some benchmarks"; some benchmarks were slower. Measured on .NET 10 NativeAOT-LLVM in two noisy runs by the PR author. Could be merged with the 1024-byte fix into one "C# modules" card.
- Sources: #5530 (v2.8.2 notes: "3-29% speedups for iteration and argument deserialization workloads").

### Operators can switch off outbound HTTP from modules — MEDIUM
- What changed (exact, verified): a new standalone config table in `{data-dir}/config.toml`:
  ```toml
  [module-http]
  enabled = false
  ```
  `ModuleHttpConfig` in `crates/core/src/config.rs` defaults to `enabled: true`. When disabled, module HTTP requests fail with the error text `module outbound HTTP requests are disabled` (`crates/core/src/host/instance_env.rs`). Per the docs: it "blocks outbound requests from procedures and module HTTP handlers. Inbound HTTP handlers remain unaffected. Changing this setting requires restarting the server."
- Before this range: no switch; module outbound HTTP was always allowed.
- Headline idea: "Lock down outbound traffic"
- One-sentence description: Self-hosted operators can now forbid modules from making outbound web requests with a single config line.
- Visual idea: A server box with outbound arrows to cloud icons, a toggle flips `enabled = false` and the arrows are cut by a shield. Inbound arrows keep flowing.
- Caveats: standalone/self-hosted only (`spacetime start` config). Don't imply Maincloud users control it. The default is still enabled. It's an operator/security feature, so it's weaker for a game-dev audience.
- Sources: #5774. `v2.9.0:crates/standalone/config.toml`, `v2.9.0:docs/docs/00300-resources/00200-reference/00100-cli-reference/00200-standalone-config.md`.

### Repeating schedules keep time (no drift) — MEDIUM
- What changed (exact, verified): in `crates/core/src/host/scheduler.rs`, interval-scheduled functions are now rescheduled from their *intended* time (`next_interval_reschedule(last_intended_at, interval)`) instead of from `Timestamp::now()` at execution. If a run is late, missed ticks are skipped to the next tick after now (like `MissedTickBehaviour::Skip`).
- Before this range: every run was scheduled from its actual execution time (verified at v2.8.0: `let reschedule_from = (Timestamp::now(), Instant::now());` in `scheduler.rs` lines ~534 and ~577), so small timer delays or a slow previous run pushed all later runs permanently later.
- Headline idea: "Timers that stay on beat"
- One-sentence description: Repeating scheduled reducers and procedures now stay locked to their original rhythm instead of slowly drifting later.
- Visual idea: Two metronomes/tick timelines. The old one's ticks drift progressively right of the grid lines; the new one stays on the grid, and a late tick skips ahead rather than shifting everything.
- Caveats: applies to interval (repeating) schedules. Missed ticks are skipped, not replayed. Don't claim hard real-time precision.
- Sources: #5735. v2.8.3 notes.

### Renaming a table accessor no longer breaks the module — LOW
- What changed: auto-migration now detects changes to a table's accessor name (Rust `accessor = …`, C# `Accessor = …`; the v2.8.2 notes call it "the `name` parameter in `#[table]` or `[Table]`") and updates the `st_*_accessor` system tables (`crates/schema/src/auto_migrate.rs`, datastore).
- Before: the rename published, then the module panicked at runtime on the next call (issue #5132).
- Caveat: that's the accessor, not the table's canonical name.
- Sources: #5544, v2.8.2 notes.

### C# HTTP requests: explicit timeouts no longer capped at 0.5 s — LOW
- What changed: `HttpClient.MaxTimeout` in `crates/bindings-csharp/Runtime/Http.cs` went from `TimeSpan.FromMilliseconds(500)` to 180 seconds, matching the host's `HTTP_MAX_TIMEOUT`.
- Caveat (release notes overstate): the v2.8.2 notes say requests "would time out prematurely for any request exceeding half a second". In the v2.8.0 code the clamp only applied when the module **set an explicit `Timeout`** (`if (timeout is not null) ... if (timeout.Value > MaxTimeout) timeout = MaxTimeout;`). Requests with no timeout used the host default of 30 s (`HTTP_DEFAULT_TIMEOUT`). The 500 ms clamp dates from #3944 (2026-01-06).
- Sources: #5751, `v2.8.0:crates/core/src/host/instance_env.rs` (lines ~920, 1016, 1022).

### TS SDK: connection no longer stalls on iOS decompression errors — LOW
- What changed: `WebsocketDecompressAdapter` (`crates/bindings-typescript/src/sdk/websocket_decompress_adapter.ts`) now catches decompress failures, logs them and closes the socket. This was seen on WebKit/Chrome iOS as `TypeError: Incomplete compressed input.`
- Before: an unhandled promise rejection, the frame dropped, `onDisconnect` never fired and the client stalled. Long-standing (the adapter dates from v1.4.0).
- Caveat: "reconnect takes over" only for apps using the React/Svelte/Solid providers' ConnectionManager. Otherwise the app just gets `onDisconnect`.
- Sources: #5668.

### Smaller fixes (list only) — LOW
- C++ multi-column index queries with 3+ columns: exact prefix + optional terminal range now work; a non-terminal range (e.g. `(exact, Range<>, exact)`) is rejected at compile time, where before it failed host-side. Also fixes index-iterator buffering for large results. (#5565)
- WebSocket idle handling: any data from the client counts as activity. On idle timeout the server sends a proper close with reason `idle timeout` and forces teardown after a 10 s grace (`SERVER_CLOSE_GRACE`). Defaults: `ping-interval = "15s"`, `idle-timeout = "30s"` under `[websocket]`. (#5517; the v2.9.0 `config.toml` comment still says send progress counts, but the PR says it doesn't.)
- C# SDK cleans up after errors/disconnects: clears outstanding request tracking, removes failed subscriptions, and a one-off query while disconnected now throws `InvalidOperationException("Cannot run one-off query, not connected to server!")`. (#5794)
- .NET host handling: `spacetime dev` picks the .NET default by host OS, and `build`/`dev`/`init`/`publish` fail early with `NativeAOT-LLVM in only supported on Windows and Linux (.NET 10).` (typo "in" is in the code). Also fixes duplicate `<PackageReference>` in .NET 8 NativeAOT projects. (#5571)
  - Before: `spacetime init` already defaulted to .NET 8 on macOS since #4915 (in v2.7.0-hotfix3).
  - **Regression introduced here (fixed in v2.10.0 by #5867):** in v2.9.0, `spacetime dev` always calls `init::resolve_default_dotnet_major()` and passes the result on as a .NET version, even for Rust/TS projects. There are two symptoms:
    - (a) On macOS, every `spacetime dev` run prints `Warning: NativeAOT-LLVM does not support macOS hosts, so this C# project will target .NET 8. .NET 8 support will be deprecated soon.` This happens even for non-C# projects. On a host with only the .NET 8 SDK installed, any OS prints `Warning: Only the .NET 8 SDK is installed, so this C# project will target .NET 8. ...`.
    - (b) On any OS, when `spacetime dev` offers to scaffold a new non-C# project, `init` fails with `--dotnet-version is only supported for C# projects (--lang csharp)`.
    - Don't feature `spacetime dev` with a new Rust/TS project, or on macOS, in a 2.9 demo.
- TS modules: `array<u8>` values read from a table scan no longer silently change when a later scan reuses the buffer (`readUInt8Array` returns a copy). (#5491)
- Deadlock fix when removing a subscription on the legacy v1 WebSocket protocol. (#5666)

## Left out (not user-visible or not promo-worthy)
- #5529 Update #2149 to post .NET 10: internal C# cleanup (callback IDs uint→int, ABI-neutral).
- #5611 per-database HTTP egress metric, #5775 egress middleware on the MCP route: Prometheus metrics only ("not billed metrics").
- #5765 add cause to init failures: metric label only (`cause` = `out_of_energy`/`other`).
- #5824 metric for unexpected module host exits, #5825 procedure metrics attribution: metrics.
- #5640 audit of warn!/error!: logging levels only.
- #5809 commitlog byte offset in decode errors: diagnostic detail in error text for operators.
- #5770 absent pages in `Table`/snapshots: preparation for freeing pages. No behavior change yet (a new snapshot sentinel, not reachable).
- #5817 move codegen git hash out of lib: build-time only.
- "Fix replication follower resync tracking" (v2.8.1 notes, no public PR): replication is not in the open-source standalone path.
- #5583 LLM benchmark evals, #5576 C# LLM benchmark fixes: benchmark tooling (#5583 also improved server skills; mentioned under plugins).
- #5728, #5722, #5729, #5516, #5732, #5631, #5819: docs-only.
- #5697, #5657, #5676, #5659, #5675, #5633, #5674, #5664, #5655, #5739, #5734, #5687, #5705, #5718, #5710, #5719, #5689, #5700, #5758, #5815, #5797, #5780, #5753, #5716, and the version bumps #5683/#5752/#5764/#5833: CI, release tooling, codeowners, deps.

## Open doubts
- How Maincloud responded to MCP calls in the 2.8.1–2.9.0 era (the route is in the open-source client-api, but availability is a Maincloud deployment decision). The release notes explicitly say "not yet available for Maincloud", so don't show Maincloud in the 2.9 MCP scene.
- Whether the plugins installed by users in September 2026 match v2.9.0 is also open. Plugins install from the repo's default branch, not the tag.
- The C# buffer-reuse numbers come from the PR author's two noisy runs, and some benchmarks regressed. Treat them as indicative.
- #5517's actual effect on disconnects is unproven. The author says "I am less confident it's the disconnection silver bullet".
- #5867 (fixed in 2.10.0): the macOS warning is confirmed in code (`v2.9.0:crates/cli/src/subcommands/init.rs` ~line 1640). The additional hard error on the `spacetime dev` → new non-C# project path also appears in code on any OS. Both were read from code, not reproduced by running.
