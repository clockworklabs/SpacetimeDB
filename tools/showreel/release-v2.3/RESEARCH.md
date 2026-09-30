# v2.3 research (v2.2.0 → v2.3.0)

Method: GitHub release notes for v2.3.0, `git log v2.2.0..v2.3.0` (58 commits: 54 PR merges on master up to
`d62295d89c`, plus 4 version-bump commits on the `bfops/bump-versions` release branch), every PR body/file list,
and the code at both tags (`git show "v2.2.0:…"` / `git show "v2.3.0:…"`). v2.2.0 is an ancestor of v2.3.0.

## Releases in range
- **v2.3.0**, published 2026-05-27 (tag commit `aa73d1c35b`, 2026-05-26). Release-note headline: first-party Godot support, faster WebSocket transport, commitlog work.
- (**v2.3.0-hotfix1**, lightweight tag `ef9404b82c`, 2026-05-27, **no GitHub release**. It only reverts #4884, an energy-accounting change for JS modules that shipped in v2.2.0. It is not an ancestor of v2.4.0, and its Cargo version is still `2.3.0`. It looks like a server-side/Maincloud hotfix. It is covered in the v2.4 research, not here.)

## Candidates

### Godot SDK + Blackholio tutorial — HIGH
- What changed (exact, verified):
  - New NuGet package **`SpacetimeDB.ClientSDK.Godot`** (`sdks/csharp/SpacetimeDB.ClientSDK.Godot.csproj`, `<Title>SpacetimeDB Godot SDK</Title>`, `Sdk="Godot.NET.Sdk/4.6.2"`, `net8.0`, `DefineConstants GODOT`). nuget.org lists **2.3.0 as the first version**, published 2026-05-27.
  - Godot-specific SDK code:
    - `sdks/csharp/src/STDBUpdateManager.cs` is a `Node` singleton that is added to the scene root and ticks registered `IDbConnection`s, so you don't write the frame loop yourself.
    - `GodotDebugLogger.cs` logs through Godot.
    - Token storage in `AuthToken.cs` was adapted for Godot.
  - New docs tutorial "Godot Tutorial" (`docs/docs/00100-intro/00300-tutorials/00500-godot-tutorial/`, slug `/tutorials/godot`) in 4 parts: Setup, Connecting to SpacetimeDB, Gameplay, Moving and Colliding. It builds "Blackhol.io", an agar.io-like game.
    - The index page says the server module can be written in **Rust or C#**. The part-2 tabs also include C++ (`spacetime init --lang cpp --server-only blackholio`).
  - A Godot demo client was added at `demo/Blackholio/client-godot/`.
  - Install line from the tutorial: `dotnet add package SpacetimeDB.ClientSDK.Godot`.
- Before this range: nothing. There was no Godot package and no "godot" mention anywhere in `docs/` or `sdks/` at v2.2.0. Godot users could only try the generic C# SDK on their own.
- Headline idea: "Now shipping for Godot." / "Godot, meet SpacetimeDB."
- One-sentence description: Godot developers can now add SpacetimeDB to a C# Godot project with one NuGet package and follow an official step-by-step tutorial to build a multiplayer game.
- Visual idea:
  - Split screen. On the left, a Godot editor mock: the FileSystem dock, `GameManager.cs`, and a terminal typing `dotnet add package SpacetimeDB.ClientSDK.Godot`.
  - On the right, the Blackholio arena: circles eating food, and a second client window mirroring the same state.
  - End card: "Godot 4.6.2 .NET · Rust / C# modules".
- Caveats (don't claim):
  - It requires the **.NET (C#) build of Godot, 4.6.2 or later**. There is no GDScript SDK; don't show GDScript.
  - The tutorial says versions before 4.6.2 are unsupported. The PR tested Windows and Linux builds only, so don't claim web, mobile or console exports.
  - The v2.3.0 Godot demo is the basic tutorial game. Username entry, leaderboard, the split mechanic and the starfield visuals landed in **v2.4.0** (#5030), so don't show a leaderboard or splitting in the 2.3 video.
  - The release note says "complete C# SDK integration". What actually shipped is the existing C# SDK plus a Godot packaging and update-manager layer. Fine to say "official Godot SDK", but avoid "native" or "GDExtension".
- Sources:
  - #4920 "Godot SDK and Blackholio tutorial".
  - `v2.3.0:sdks/csharp/SpacetimeDB.ClientSDK.Godot.csproj`.
  - `v2.3.0:docs/docs/00100-intro/00300-tutorials/00500-godot-tutorial/index.md` and `00200-part-1.md`.
  - nuget.org registration for `spacetimedb.clientsdk.godot`.
  - Release note: "SpacetimeDB now officially supports Godot with a complete C# SDK integration."

### "Pipeline everything": faster server hot path — MEDIUM
- What changed (exact, verified):
  - **#4962 (JS modules) and #4973 (WASM modules):** the WebSocket receive loop no longer waits for each request to finish before handing the next one to the database.
    - In `v2.2.0:crates/client-api/src/routes/subscribe.rs`, `ws_recv_task` did `message_handler(data, timer).await` per message, and the handler awaited completion.
    - At v2.3.0, `ClientConnection` calls enqueue-only methods (`enqueue_reducer`, `enqueue_procedure`, …).
    - Reducers go through a single serialized queue to one instance thread. Procedures use their own pool.
  - **Side effect worth telling:** a procedure that yields (for example, while waiting on an outbound HTTP call) no longer blocks later requests from the same client.
  - **#5051:** the WebSocket send path processes messages in batches (`recv_many`).
  - **#5061:** the server now coalesces several server→client messages into one WebSocket frame when the client negotiated protocol `v3.bsatn.spacetimedb`. `spacetime subscribe` now uses v3 and falls back if negotiation fails.
  - **#5018:** only the modified pages of the commitlog offset index are `msync`ed, not the whole file, on each transaction.
  - **#5074** raised the default commitlog `write-buffer-size` from 8 KiB to 128 KiB.
- Before this range:
  - The v3 WebSocket protocol already existed at v2.2.0 (`crates/client-api-messages/src/websocket/v3.rs`, added in #4761), and the TS SDK already preferred it (`PREFERRED_WS_PROTOCOLS = [V3_WS_PROTOCOL, V2_WS_PROTOCOL]`).
  - What is new is that the server batches its responses and that the pipeline no longer waits on each request.
- Rating note: this could carry a full scene only as a generic "faster" scene with no numbers.
- Headline idea: "Less waiting. More throughput."
- One-sentence description: The 2.3 server no longer waits for each request to finish before starting the next one from the same client, and it sends outgoing messages in batches, cutting overhead for busy real-time apps. For TypeScript-SDK clients, several replies can also share one network message.
- Visual idea:
  - Before/after conveyor belt. Before: one box at a time, with a stop-and-wait gap between boxes. After: a continuous stream with boxes grouped into crates (batched frames).
  - A small side vignette: a slow "procedure" box moves onto a side track while reducer boxes keep flowing.
- Caveats (don't claim):
  - **No before/after numbers exist.** `templates/keynote-2/README.md` changed from 107,850 TPS (v2.2.0) to 279,024 / 303,919 TPS (v2.3.0), but the methodology changed completely: 50 connections before; 64 clients with pipelining 40 and 300 s runs after. It is **not a like-for-like speedup**, so don't present "2.6× faster".
    - If a number is needed, you can say: "~280k–300k transfers/s on a single standalone node in our keynote-2 benchmark (64 clients, pipelined)", from `v2.3.0:templates/keynote-2/README.md`.
  - The v3 response batching only helps clients that speak v3. At v2.3.0 that is the **TypeScript SDK** and `spacetime subscribe`. The Rust SDK (`sdks/rust/src/websocket.rs`), the C# SDK (`SpacetimeDBClient.cs`) and the Unreal SDK still request `v2.bsatn.spacetimedb`. Don't say "all SDKs".
  - Behavior change (from the #4962 body): procedures may now complete **out of order** relative to other messages from the same connection. Don't claim strict per-connection ordering for procedures.
  - The release note line "Compression deferred when under write load" is wrong. #4974 was reverted by #4987 four days later, net zero. Don't use it.
- Sources:
  - #4962, #4973, #5051, #5061, #5018, #5074.
  - `v2.2.0:` vs `v2.3.0:crates/client-api/src/routes/subscribe.rs`.
  - `v2.3.0:crates/bindings-typescript/src/sdk/websocket_protocols.ts`.
  - `v2.3.0:templates/keynote-2/README.md`.

### Vue `useProcedure` + TanStack `useProcedure` — MEDIUM
- What changed (exact, verified):
  - New `useProcedure` composable in `spacetimedb/vue` (`crates/bindings-typescript/src/vue/useProcedure.ts`). It mirrors React's hook: it returns an async function that calls the procedure, and it queues calls made before the connection is active.
  - `spacetimedb/tanstack` now also exports `useProcedure` (re-exported from the React implementation).
- Before this range: at v2.2.0, only `spacetimedb/react` had `useProcedure`. The Vue integration had no procedure hook, and the TanStack entry point did not export it (issue #4957).
- Headline idea: "Procedures, now in Vue."
- One-sentence description: Vue and TanStack apps can now call server procedures with a single typed `useProcedure` hook, just like React apps already could.
- Visual idea: a Vue SFC snippet `const sendMessage = useProcedure(procedures.sendMessage)`. This syntax is verified from React usage in `v2.4.0:templates/llm-chat-ts/src/App.tsx`; the Vue signature is identical, and generated bindings export `procedures`., then a button click and the typed result appearing. Could be a "plus" card with Vue and TanStack logos.
- Caveats (don't claim): Svelte and Angular did **not** get `useProcedure` in this release. It is not a new capability for React. Procedures themselves are not new.
- Sources: #4999, #4984, `v2.3.0:crates/bindings-typescript/src/vue/useProcedure.ts`, `v2.3.0:crates/bindings-typescript/src/tanstack/index.ts`.

### Unity 6 WebGL builds connect — MEDIUM
- What changed (exact, verified): the C# SDK's `WebSocket.jslib` now routes callbacks through a `$WebSocketDynCall` helper. It uses `getWasmTableEntry(ptr)` when available (Unity 6+) and falls back to legacy `dynCall` on Unity 2022 and earlier.
- Before this range: Unity 6 WebGL builds failed with `dynCall is not defined` (issue #4959), because Unity 6's newer Emscripten removed `dynCall`. The PR shows Unity 6000.4.5f1 WebGL as "previously failing".
- Headline idea: "Unity 6 on the web. Connected."
- One-sentence description: Games built with Unity 6 can now connect to SpacetimeDB from a web (WebGL) build, and older Unity versions keep working.
- Visual idea: browser window with a Unity WebGL player. The console shows red `dynCall is not defined`, which wipes to a green "Connected · subscribed" and rows streaming in.
- Caveats (don't claim):
  - It is a WebGL-only fix. Unity 6 Editor and desktop builds were not broken.
  - The release note says "C# modules and clients". It is only the **client** SDK (a jslib plugin), not server modules.
  - Tested on Unity 2022.3.62f2 and 6000.4.5f1.
- Sources: #4961, `sdks/csharp/src/Plugins/WebSocket.jslib` at v2.3.0.

### `spacetime init` AI-assistant rules rebuilt from per-language skills — LOW
- What changed (exact, verified):
  - `spacetime init` still writes AI-assistant files: `.cursor/rules/*.mdc`, `CLAUDE.md`, `AGENTS.md`, `.windsurfrules` and `.github/copilot-instructions.md`.
  - The content now comes from embedded `skills/*/SKILL.md`:
    - always `concepts` and `cli`;
    - plus the server-language skill (`rust-server`, `typescript-server`, `csharp-server` or `cpp-server`);
    - C++ server projects also get `unreal`;
    - plus the client skill (`typescript-client`, or `csharp-client` and `unity`).
- Before this range: v2.2.0 embedded `docs/static/ai-rules/*.mdc` for TypeScript, Rust and C# only. C++ projects got only the base rules, and there were no Unity or Unreal rules.
- Headline idea: "Your AI pair knows SpacetimeDB." (use with care, see caveats)
- One-sentence description: New projects from `spacetime init` now come with refreshed per-language instructions for AI coding assistants, now including C++ modules and Unreal/Unity clients.
- Visual idea: terminal `spacetime init`, then a file tree highlighting `CLAUDE.md`, `AGENTS.md` and `.cursor/rules/cpp-server.mdc`.
- Caveats (don't claim): installing AI rules is **not new**; this release only changes their content and coverage. There is no Rust client skill (the code comments "no Rust client skill yet"). It shipped inside #4740, which is mostly LLM-benchmark tooling; the release notes don't mention it.
- Sources: #4740, `v2.3.0:crates/cli/src/subcommands/init.rs` (`install_ai_rules`), `v2.3.0:crates/cli/build.rs` (`get_skill`).

### Persistent `listen_addr` for `spacetime start` — LOW
- What changed: `cli.toml` accepts a top-level `listen_addr = "0.0.0.0:4000"`. `spacetime start` uses it unless `--listen-addr`/`-l` is passed. Precedence: flag, then `cli.toml`, then the built-in `0.0.0.0:3000`.
- Before: only the flag existed. (#4576, the original `spacetime.json` variant, was never released.)
- Sources: #4900, `v2.3.0:crates/cli/src/config.rs` (`LISTEN_ADDR_KEY`), `v2.3.0:crates/cli/src/subcommands/start.rs`, CLI reference text "Set a persistent default listen address in cli.toml".

### Self-hosting config knobs — LOW
- What changed: the standalone `config.toml` gained several sections.
  - `[commitlog]` with `max-segment-size`, `write-buffer-size` (default now 131072), `preallocate-segments`, `offset-index-interval-bytes`, `offset-index-require-segment-fsync` and `log-format-version` (#5074).
  - `[v8] procedure-instance-pool-size` and `[wasm] procedure-instance-pool-size`, which default to the number of cores (#4962/#4973).
- Caveat: the release note calls these "commitlog compression knobs". **None of them are compression settings.**
- Sources: `git diff v2.2.0 v2.3.0 -- crates/standalone/config.toml`, #5074.

### `ReducerContext::identity` → `database_identity` — LOW
- What changed:
  - Rust: `ctx.database_identity()` added; `identity()` kept with `#[deprecated(note = "Use `ReducerContext::database_identity` instead.")]`.
  - C#: `DatabaseIdentity` added; `Identity` marked `[Obsolete("ReducerContext.Identity is deprecated. Use DatabaseIdentity instead.")]`.
- Purpose: avoid confusing the module's identity with the caller's (`ctx.sender`, issue #3201).
- Caveat: Rust and C# only (not TS or C++). The old names still work, so nothing breaks.
- Sources: #4843, `v2.3.0:crates/bindings/src/lib.rs` ~l.1084–1100.

### Rust modules: `DbContext::db_read_only()` — LOW
- What changed: new method `db_read_only(&self) -> &LocalReadOnly` on the module-side `DbContext` trait, gated behind the `unstable` feature. It lets one generic helper read tables from reducer, view, anonymous-view and procedure-transaction contexts.
- Caveat: the release note says "code reuse between client and server contexts". That is **wrong**: this is inside modules only (`crates/bindings`), and unstable.
- Sources: #4707, `v2.3.0:crates/bindings/src/lib.rs`.

### Rust client SDK fixes — LOW
- #4935: failures before the connection is established now fire `on_connect_error` instead of `on_disconnect`, and cancelling while connecting is no longer reported as an error.
- #4938: `unsubscribe_then` returns `Err(Error::Internal(..))` instead of panicking when the connection has already shut down.
- Both are long-standing Rust SDK bugs (not introduced in this range).
- Sources: #4935, #4938, `sdks/rust/src/db_connection.rs`.

### Server/runtime bug fixes — LOW
- **#4986:** V8 segfault in the v1 JS-module code path, from an uninitialized `RECV_SLOT_INDEX` embedder slot. It was introduced by #4302, which is in v2.0.0, so it is real news for 2.2 users running older-ABI JS modules.
- **#4985:** republishing a module whose view's exported (canonical) name differs from its accessor no longer plans a false `RemoveView` + `AddView` (issue #4842, reported on 2.1.0, affected TS and Rust).
- **#4939:** snapshots on Windows failed with "access denied" because of directory fsync. The bug was introduced by #4891 in **v2.2.0**, so the fix is news for 2.2 Windows users.
- **#4863:** a commitlog left with an empty (header-only) tail segment can be reopened for writing.
- **#5000:** invalid-issuer JWTs no longer return a stack trace in the HTTP error body (issue #4960).

### `spacetime version uninstall` clear error — LOW
- `spacetime version uninstall 2.0.3` for a version that isn't installed now fails before the prompt with `Error: v2.0.3 is not installed`, instead of asking for confirmation and then printing "No such file or directory (os error 2)".
- Sources: #4774.

### HTTP/2 on the standalone server — LOW
- What changed: Axum's `http2` feature was enabled, so the standalone server accepts cleartext HTTP/2 (h2c) from clients that use prior knowledge (`curl --http2-prior-knowledge http://127.0.0.1:3000/v1/ping`). HTTP/1.1 is unchanged.
- Caveats (don't claim):
  - The release note says "more efficient client connections with multiplexed streams". Overstated: the PR itself says `https://maincloud.spacetimedb.com` still negotiates **http/1.1** at the edge.
  - No SDK was changed to use HTTP/2, and WebSocket subscriptions are unaffected.
- Sources: #5027 body.

### Smaller items — LOW
- #5001: dropped `serde_json/arbitrary_precision` from the workspace, so it is no longer forced on downstream Rust crates through feature unification (issue #4989).
- #5077: install/upgrade binaries are downloaded from AWS instead of DigitalOcean. The DigitalOcean mirror stops working for older clients.
- #4904: docs added "Deploy on Railway".
- #4908: docs added "Steam Session Tickets with SpacetimeAuth".
- #4917: docs visual redesign.

## Left out (not user-visible or not promo-worthy)
- #4974 Defer commitlog compression when under load: reverted by #4987 in the same range, net zero. The release note still lists it.
- #4987 Do not defer commitlog compression: the revert above.
- #4981 commitlog: Don't lock while compressing: internal locking, no user-facing effect beyond general throughput.
- #4708 Compression stats and commitlog compression function: internal API (bandwidth-limiting hook).
- #4979 Rollback prepared statements: reverts "Use prepared statements for postgres keynote benchmark" (#4522), so it is benchmark tooling only. **The release note's "Prepared statements are now properly rolled back on transaction failure" is false.**
- #4990 / #5062 / #5098 Namespaces (recursive mounts): added, then fully reverted within the range (`mounts` is absent at v2.3.0).
- #5032 / #5084 minimum pnpm package age: CI. The `.npmrc` files it added to templates were removed again; no template ships an `.npmrc` at v2.3.0.
- #4982 Abstract SnapshotWorker/durability::Local: internal refactor for simulation testing.
- #3838 commitlog write-throughput benchmarks, #4967 remove distributed benchmark harness, #4975 / #4997 keynote benchmark README and methodology: benchmark tooling. Only usable as a source of figures, with caveats (see above).
- #4740 LLM benchmark improvements: benchmark and CI tooling, except the `spacetime init` skills change listed above.
- #4972 `cargo ci dlls` → `cargo regen csharp dlls`: contributor tooling only.
- #4928, #4929, #4942, #4950, #4956, #4977, #5096: CI, scripts, warnings.
- #4963, #4952, #5101: docs wording, links, CSS fix.
- #5120 / bump-versions commits: version bump.

## Open doubts
- Maincloud rollout: none of the server-side items can be tied to a Maincloud deploy date from the repo.
- The Godot SDK's platform coverage beyond Windows/Linux desktop (macOS, Android, iOS) is untested per the PR. Keep visuals desktop-only.
- Safari: the TS SDK shipped in **v2.2.0 and v2.3.0** uses `for await` over a `ReadableStream`, which older Safari lacks, with default `gzip` compression. Compressed messages likely failed on older Safari. The fix (#5144) is in **v2.4.0**, so the 2.3 video must not claim Safari fixes.
- Out-of-order procedures (#4962) is a behavior change that some apps could notice. Worth a footnote if the "faster" scene shows procedures.
