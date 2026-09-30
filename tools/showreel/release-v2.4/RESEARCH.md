# v2.4 research (v2.3.0 → v2.4.0)

Method: GitHub release notes for v2.4.0 (plus v2.4.1 only to exclude it), `git log v2.3.0..v2.4.0` (27 commits), every
PR body/file list, and the code at both tags.

Topology note: `v2.3.0` sits on a release branch that forked master at `d62295d89c` (2026-05-23), so it is **not** an
ancestor of `v2.4.0`. `git log v2.3.0..v2.4.0` is therefore every master commit after that fork point. `v2.3.0-hotfix1`
is also off-branch; its single change (the #4884 revert) reached master as #4927, which is in this range.

## Releases in range
- **v2.3.0-hotfix1**: lightweight tag `ef9404b82c`, 2026-05-27, on branch `release/v2.3.0-hotfix1`. **No GitHub release**, and the Cargo version is still `2.3.0`. Its only content is a manual cherry-pick of #4927, which reverts #4884 ("Properly handle execution time<->energy conversion in v8 host").
- **v2.4.0**: published 2026-06-03 (tag commit `b3547448cf`, 2026-06-02). Release-note headline: HTTP handlers in modules, faster WASM reducer execution, durability fixes.
- (Out of range: **v2.4.1**, 2026-06-05, adds primary keys for procedural views (#5111) and an index-schema fix (#5145). Don't use these in the 2.4 video.)

## Candidates

### HTTP handlers: modules serve their own HTTP routes — HIGH
- What changed (exact, verified): a module can define HTTP handlers and register them on a router. Incoming HTTP requests to `/v1/database/:name_or_identity/route/{*path}` are dispatched to them. The handler gets a context that can open a read/write transaction (`with_tx`), use the RNG/UUIDs, and, verified in Rust, make outbound HTTP calls (`ctx.http`; not checked for TS/C#/C++).
  - **Rust:**
    - `#[spacetimedb::http::handler]` on `fn(&mut HandlerContext, Request) -> Response`.
    - `#[spacetimedb::http::router] fn router() -> Router { Router::new().get("/say-hello", say_hello) }`.
    - The whole `spacetimedb::http` module is behind the crate feature `unstable`: `spacetimedb = { version = "2.*", features = ["unstable"] }`.
  - **TypeScript:**
    - `export const say_hello = spacetimedb.httpHandler((ctx, req) => new SyncResponse("Hello!"))`.
    - `export const router = spacetimedb.httpRouter(new Router().get("/say-hello", say_hello))`.
    - Imports come from `spacetimedb/server`. There is no opt-in flag in TS; the docs page just says beta.
  - **C#:**
    - `[SpacetimeDB.HttpHandler]` methods `(HandlerContext ctx, HttpRequest request) -> HttpResponse`.
    - `[SpacetimeDB.HttpRouter] public static Router Router() => SpacetimeDB.Router.New().Get("/say-hello", …)`.
    - Needs `#pragma warning disable STDB_UNSTABLE`.
  - **C++:**
    - `SPACETIMEDB_HTTP_HANDLER(name, HandlerContext ctx, HttpRequest request)` and `SPACETIMEDB_HTTP_ROUTER(router)`.
    - Needs `SPACETIMEDB_UNSTABLE_FEATURES`; the header `#error`s without it.
  - Router methods in all languages: `get`, `head`, `options`, `put`, `delete` (`delete_` in C++), `post`, `patch`, `any`, `nest(prefix, sub)` and `merge(other)`.
  - Routing is strict: exact path match, and trailing slashes matter.
  - Docs page: `docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md` (slug `/functions/http-handlers`), with the banner "HTTP handlers are currently in beta, and their API may change".
- Before this range: nothing. Modules could only be reached over the WebSocket or the fixed HTTP API (call reducer, SQL, …). Procedures have been able to make **outbound** HTTP requests since v1.10; **inbound** custom routes are new.
- Headline idea: "Your database speaks HTTP." / "Webhooks, straight into your database."
- One-sentence description: A SpacetimeDB module can now expose its own HTTP endpoints, so services like payment providers, bots or plain `curl` can call your database directly and read or write tables in a transaction.
- Visual idea:
  - Three-pane scene. Pane 1: a short Rust or TS module with `#[handler] fn insert` and `Router::new().post("/insert", insert)`.
  - Pane 2: a terminal running `curl -X POST https://…/v1/database/my-db/route/insert -d 'hello'`, which prints `1`.
  - Pane 3: the live table gains a row, and a WebSocket-subscribed client UI updates at the same moment.
  - Alternative: a "webhook" icon (Stripe-like or GitHub-like, but unbranded) arrowing into the database.
- Caveats (don't claim):
  - **It is beta/unstable.** It needs opt-in in Rust (`features = ["unstable"]`), C# (`#pragma warning disable STDB_UNSTABLE`) and C++ (`SPACETIMEDB_UNSTABLE_FEATURES`). Say "beta" or "preview" on screen.
  - **No built-in auth.** Requests bypass SpacetimeDB's auth middleware; `Authorization` headers are passed through untouched, and there is no `sender`/caller identity on `HandlerContext` (`ctx.identity()` is the *database's* identity). Don't imply requests are authenticated or tied to a user identity.
  - Paths are restricted to ASCII letters, digits and `-_~/`. There are no path parameters or wildcards yet, so don't show `/users/:id`.
  - The PR notes that energy accounting and timing metrics are not wired up for handlers. Don't mention limits or billing.
  - Everything lives under `/v1/database/<db>/route/…`, so you can't serve at the domain root. Don't claim "host your website" or custom domains.
  - The release note's Rust snippet says `version = "1.*"`. That's a typo; use `"2.*"`.
  - The release note cites TS #4980, C# #5024 and C++ #5023 as separate PRs. Those merged into a feature branch; everything landed on master in the single squash **#4636**. All four languages are present at `v2.4.0` (verified: `crates/bindings-typescript/src/server/http_handlers.ts`, `crates/bindings-csharp/Runtime/Router.cs`, `crates/bindings-cpp/include/spacetimedb/http_handler_macros.h`).
  - Maincloud availability can't be verified from the repo (see Open doubts).
- Sources:
  - #4636 body.
  - `v2.4.0:docs/docs/00200-core-concepts/00200-functions/00600-HTTP-handlers.md`.
  - `v2.4.0:crates/bindings/src/http.rs` (`HandlerContext`, `Router`).
  - `v2.4.0:crates/client-api/src/routes/database.rs` (`/:name_or_identity/route/*path`).
  - `v2.4.0:crates/bindings-csharp/Runtime/Attrs.cs` (`HttpHandlerAttribute`, `HttpRouterAttribute`).
  - Release note: "Modules can now define custom HTTP routes and serve arbitrary HTTP requests directly from module code".

### Three new `spacetime init` templates — MEDIUM
- What changed (exact, verified): three new templates are embedded in the CLI. Templates are compiled into the binary from `templates/*/.template.json` by `crates/cli/build.rs`, so you need the v2.4.0 CLI. All three are TypeScript module + React client:
  - `llm-chat-ts`: "Simple TypeScript chat app that calls an LLM API from a SpacetimeDB module". It is a small ChatGPT-style clone.
    - Each user sets an OpenRouter or OpenAI key and model; the default is `openrouter` with `openai/gpt-4o-mini`.
    - Chats and messages are private per identity. The module calls the LLM from **procedures**.
  - `hangman-react-ts`: "Competitive Hangman game with React and TypeScript server". One shared game for all players.
  - `money-exchange-react-ts`: "Private account money exchange demo with React and TypeScript server". Each account starts with $100.00, has a nickname, and uses double-entry transfers.
  - Usage: `spacetime init --template <id>` (`-t`), or `spacetime dev --template llm-chat-ts`, as the llm-chat README shows. `dev` also has `--template` at v2.4.0 (`crates/cli/src/subcommands/dev.rs`).
  - The `tags: ["Launchpad"]` field on two of the templates is not read by the CLI (`build.rs` and `init.rs` don't reference `tags`), so all three appear equally in `init`.
- Before this range: the v2.3.0 template list was angular-ts, astro-ts, basic-cpp/cs/rs/ts, browser-ts, bun-ts, chat-console-cs/rs, chat-react-ts, deno-ts, nextjs-ts, nodejs-ts, nuxt-ts, react-ts, remix-ts, svelte-ts, tanstack-ts and vue-ts. There were no LLM, game or money templates.
- Headline idea: "Start from a working app."
- One-sentence description: `spacetime init` now offers three ready-to-run starter apps: an AI chat app, a multiplayer Hangman game and a money-transfer demo.
- Visual idea: terminal `spacetime dev --template llm-chat-ts`, then a browser at `localhost:5173` with a chat thread: the user message appears, and after a beat the full assistant reply pops in. **No token streaming**: the `send_message` procedure inserts the user row, calls the LLM with `http.fetch`, then inserts the whole assistant reply in one row. Quick cuts to the Hangman board and the transfer UI. Works as a "plus" card trio.
- Caveats (don't claim):
  - The LLM template stores the user's API key as **module data**. Its README says: "This template is not a secret manager". Don't claim secure key storage.
  - You bring your own OpenRouter/OpenAI key; there is no built-in AI.
  - All three are TypeScript/React only.
  - The money demo is play money, not payments.
- Sources: #5150, #5119, #5134, `v2.4.0:templates/{llm-chat-ts,hangman-react-ts,money-exchange-react-ts}/.template.json`, `v2.4.0:templates/llm-chat-ts/README.md`, `v2.4.0:crates/cli/src/subcommands/init.rs` (`Arg::new("template").short('t').long("template")`).

### Leaner reducer execution for WASM modules (Rust / C# / C++) — MEDIUM
- What changed (exact, verified): each WASM module now gets two runtimes.
  - **Reducers** run on a dedicated synchronous wasmtime runtime backed by a single OS thread.
  - **Procedures** stay on the async runtime.
  - This removes async-call overhead from every reducer call.
- Before this range: a single async-enabled wasm runtime served all requests, even though only procedures can yield. (JS/TS modules already had a dedicated main-lane worker after #4962 in v2.3.0.)
- Headline idea: "Less overhead on every call." (mechanism, not a measured speedup)
- One-sentence description: In Rust, C# and C++ modules, the functions that change your data now run on their own dedicated thread, which removes async scheduling overhead from their path. #5095 calls itself a "pure refactor", and no benchmark result is published.
- Visual idea: a highway with a toll booth (the async scaffolding). In 2.4, reducers take an express lane with no booth, while procedures stay in the regular lane.
- Caveats (don't claim):
  - **There are no published before/after numbers.** The only committed figure is a CI gate (#5078/#5159): keynote-2 must sustain **≥ 275,000 TPS** for both the Rust and TypeScript modules on a dedicated runner (`v2.4.0:tools/ci/src/keynote_bench.rs`, `min_tps: 275_000.0`, 60 s, concurrency 64). That is a regression floor, not a speedup.
  - This does not apply to TypeScript/JS modules.
  - Don't claim "X× faster".
- Sources: #5095 body, `v2.4.0:crates/core/src/host/wasmtime/`, `v2.4.0:tools/ci/src/keynote_bench.rs`, release note "Faster WASM reducer execution".

### Godot Blackholio demo completed + Godot SDK fix — MEDIUM
- What changed (exact, verified):
  - `demo/Blackholio/client-godot/` gained username selection, a leaderboard (`HudController.cs`, `MaxLeaderboardRows = 11`), the split mechanic (`PlayerSplit` reducer bindings), respawn/suicide, consume-entity events, `StarfieldBackground.cs`, and Godot play-mode tests.
  - SDK side: `STDBUpdateManager` now adds itself to the scene tree with `CallDeferred(AddChild, …)`, which avoids adding a child while the scene tree is still being set up.
  - Separately, **#5140** added a TypeScript Blackholio (`demo/Blackholio/server-ts/` plus a browser client in `client-ts/` using Phaser 4). Its PR says "No doc updates yet".
- Before this range: the v2.3.0 Godot demo had only the tutorial-level game (move, eat, collide). There was no TypeScript Blackholio.
- Headline idea: "Blackholio, complete in Godot."
- Rating note: drop this to a list mention if the 2.3 video already covers Godot.
- One-sentence description: The Blackholio sample game is now feature-complete in Godot and has a new TypeScript server and browser client.
- Visual idea: a Godot window with a leaderboard, a player splitting into circles and a starfield. Then a browser tab running the Phaser version against the same server.
- Caveats (don't claim):
  - These are **demo projects in the repo**, not SDK features, and not tied to the 2.4 binaries.
  - There is no TypeScript Blackholio tutorial yet.
  - Only 4 lines of the Godot tutorial changed; the completed features are in the demo, not in the tutorial.
- Sources: #5030, #5140, `v2.4.0:demo/Blackholio/client-godot/HudController.cs`, `v2.4.0:sdks/csharp/src/STDBUpdateManager.cs`.

### Core Rust module crates and the Unreal SDK move to Apache 2.0 — MEDIUM
- What changed (exact, verified): the `LICENSE` symlinks changed from `../../licenses/BSL.txt` to `../../licenses/apache2.txt` for:
  - `crates/bindings`, which is the `spacetimedb` crate that Rust modules depend on;
  - `crates/bindings-sys`, `crates/bindings-macro`, `crates/lib`, `crates/primitives` and `crates/sats`.

  `sdks/unreal/LICENSE` was **added**, pointing to apache2. `crates/query-builder` was already Apache.
- Still BSL at v2.4.0 (verified): root `LICENSE.txt` (SpacetimeDB Business Source License), `crates/core`, `crates/bindings-csharp`, `crates/bindings-cpp` and `sdks/rust`. `sdks/csharp` and `crates/bindings-typescript` have no LICENSE symlink in-tree.
- Rating note: use it only with the exact wording below.
- Headline idea: "Module libraries, now Apache 2.0."
- One-sentence description: The Rust libraries your modules compile against, plus the Unreal client SDK, are now licensed Apache 2.0.
- Visual idea: a simple card listing the crate names flipping from "BSL" to "Apache-2.0".
- Caveats (don't claim):
  - **Not "SpacetimeDB is now Apache/open source".** The server (`core`) and the root license remain BSL.
  - The Rust *client* SDK (`sdks/rust`), C# module bindings and C++ module bindings did not change.
  - It is not in the release notes' Features section, so confirm the messaging with the team first.
- Sources: #5151, `git cat-file -p "v2.3.0:crates/bindings/LICENSE"` vs `"v2.4.0:…"`.

### Safari: compressed messages decode again (TypeScript SDK) — LOW
- What changed (exact, verified): `crates/bindings-typescript/src/sdk/decompress.ts` reads the decompressed stream with `getReader()` instead of `for await (… of stream)` and `Blob.bytes()`.
- Before this range: introduced by #4561 in **v2.2.0**. Older Safari lacks `ReadableStream[Symbol.asyncIterator]`, and the TS SDK's default compression is `'gzip'` (`db_connection_builder.ts`: `#compression … = 'gzip'`). So compressed (above-threshold) server messages likely failed on those Safari versions with TS SDK 2.2.0–2.3.0.
- Headline idea: "Safari, back in sync."
- One-sentence description: Web apps using the TypeScript SDK work reliably in Safari again when messages are compressed.
- Visual idea: a Safari window where a stalled list starts streaming rows again.
- Caveats (don't claim):
  - It is a fix for a regression from v2.2.0, not a new capability.
  - The exact Safari versions affected are unverified ("until recently" per the PR).
  - Chrome and Firefox were never affected.
- Sources: #5144, `v2.3.0:crates/bindings-typescript/src/sdk/decompress.ts`, `v2.3.0:crates/bindings-typescript/src/sdk/db_connection_builder.ts`.

### Crash-recovery hardening in the commitlog — LOW
- What changed (exact, verified):
  - **#5116:** when a commitlog segment is resumed after restart, it is now truncated to its validated size.
    - Previously, trailing bytes shorter than a commit header could be left behind. A later append would make the segment corrupt, and on the next restart everything written after those bytes became unreachable. This is silent data loss.
    - The segment writer also no longer opens with `O_APPEND`; that change was needed on Windows.
  - **#5129:** commit-decode errors now carry context.
- Before this range: long-standing. `resume_segment_writer` at v2.3.0 did not truncate. It was not introduced within 2.3.
- Rating note: at most a quiet "reliability" card.
- Headline idea: "Cleaner restarts after a crash."
- One-sentence description: After a crash, SpacetimeDB now trims half-written log data before it resumes writing, closing a rare case where later writes could become unreadable.
- Visual idea: a power-plug-pull animation, a restart, then a log strip with a jagged tail snipped off before new blocks append cleanly.
- Caveats (don't claim):
  - The trigger is rare: a crash leaving a partial commit header.
  - Don't frame it as "we fixed data loss" in a promo without team sign-off.
  - Don't claim zero data loss in general.
- Sources: #5116 body, #5129, release note "Durability: silent data loss on resume fixed."

### Views stay correct after a module update — LOW
- What changed (#5149): committed view read sets no longer store module-local indexes.
  - Previously, after a module update, a write that triggered a view refresh could dispatch the wrong view function or materialize into the wrong table. That led to a fatal error that could fail the calling reducer.
  - The PR says "I believe this to fix #4947". Issue #4947 is "[CRITICAL] Views recomputation can roll back transactions".
- Caveat: the fix of #4947 is not confirmed in the PR ("I believe"). Not mentioned in the release notes.
- Sources: #5149, issue #4947.

### Energy accounting for JS modules reverted — LOW
- What changed: #4927 reverts #4884.
  - JS/V8 energy is again computed with `duration_to_budget(duration)` instead of `duration × EnergyQuanta::PER_EXECUTION_NANOSEC` (2 TeV per second).
  - The PR calls it "a pricing change that we made to V8 (TypeScript) based modules".
- Correction to the release note: it says "A regression introduced in v2.3.0". **#4884 actually shipped in v2.2.0** (`git log v2.2.0 --grep="(#4884)"` finds `458eac8c85`). The same revert was hotfixed as `v2.3.0-hotfix1` on 2026-05-27, which has no GitHub release.
- Caveat: billing and pricing claims are off-limits. It only matters for hosted energy accounting.
- Sources: #4927, #4884, tag `v2.3.0-hotfix1`.

## Left out (not user-visible or not promo-worthy)
- #5122 V8 heap metrics for procedure workers: Prometheus metrics for operators only. The PR says "not a stable API".
- #5137 core: Remove view cleanup trace logs: logging.
- #5078 required keynote-2 CI check, #5159 lower its threshold: CI (the ≥275k TPS floor is cited above only as a figure source).
- #5147 Move Internal Tests workflow, #4995 skip Internal Tests for docs-only, #5160 remove `.github/docker-compose.yml`, #5152 `ci self-docs` includes value names, #5133 / #5136 Godot/Unity CI build isolation: CI.
- #4925 Add Deep Database Style: internal engineering style guide (`docs/DEEP_DATABASE_STYLE.md`).
- #5142 Add basic troubleshooting guide: docs-only (new page `docs/docs/00300-resources/00100-how-to/00050-troubleshooting.md`). Could appear in an end-card list at most.
- #5120 Bump versions to 2.3.0, #5162 Version bump 2.4.0: version bumps.

## Open doubts
- **Maincloud:** whether and when `maincloud.spacetimedb.com` served `/route/…` HTTP handlers after 2.4.0 can't be verified from the repo. The curl visual should use a local `spacetime start` host, or be confirmed with the team.
- **TypeScript HTTP handlers are not code-gated.** Rust, C# and C++ require explicit unstable opt-ins; TS relies only on the docs "beta" banner. Keep the "beta" label on screen regardless.
- **Licensing message (#5151):** confirm with the team how they want it announced. It isn't in the release notes' feature list.
- **v2.3.0-hotfix1** was probably a Maincloud-only deploy: no GitHub release, and the version is unchanged. Nothing from it should appear in either video.
- **Safari:** the exact affected Safari versions are unknown, and so is whether all compressed messages failed or only some.
