# v2.6 research (v2.5.0 → v2.6.0)

Scope note: `v2.5.0-hotfix1` (tag 2026-06-12, no GitHub release) is a one-commit branch off `v2.5.0`
("Manually apply open PR #5288: Fix commitlog replay of dropped event tables") and is **not** an ancestor of
`v2.6.0`; the same fix (#5288) is on master inside `v2.5.0..v2.6.0`. So the user-visible delta is
`git log v2.5.0..v2.6.0` (24 commits). Claims below were checked with `git show "<tag>:<path>"` at `v2.5.0` and `v2.6.0`.

**Important: the v2.6.0 release notes overstate.** Several headline items are not new in 2.6:
- "Primary Key support for Views (Rust, TypeScript, and C#)" — Rust/TS shipped in v2.4.1 (#5111), C# in v2.5.0 (#5246). Only the docs page update (#5327) is new in 2.6.
- "Commitlog configuration knobs" (#5074) and "Client binaries from DigitalOcean -> AWS" (#5077) — both merged 2026-05-20 and already contained in `v2.3.0` (`git tag --contains`).
- "Allow layout-altering automigrations of event tables" (#5269) — in v2.5.0.
- "Cross compile CLI binaries for ARM" (#5176) — CI-only; aarch64 Linux binaries already existed, now built on x86 runners.
- "docs: clarify deterministic reducer randomness" (#5322) — PR is **not merged** (mergedAt null) and not in v2.6.0.

## Releases in range
- `v2.5.0-hotfix1` — tag 2026-06-12 (no GitHub release). Fix: databases that dropped an event table on 2.5.0 could not restart (#5288).
- `v2.6.0` — GitHub release 2026-06-16 (tag commit 2026-06-16). Notes headline view primary keys (not new, see above); actual new user-visible items: C++ query builder, React provider auto-reconnect, C# Timestamp primary keys, event-table drop/restart fixes, scheduler resilience, `spacetime subscribe -n` strictness.

## Candidates

### React apps reconnect by themselves — HIGH
- What changed (exact, verified): #5185. `crates/bindings-typescript/src/sdk/connection_manager.ts`: when the WebSocket of a connection managed by `SpacetimeDBProvider` closes or errors while the provider is still mounted, the manager schedules a rebuild with exponential backoff (`CONNECTION_MANAGER_RECONNECT_BASE_DELAY_MS = 1000`, doubling per consecutive failure, capped at `CONNECTION_MANAGER_RECONNECT_MAX_DELAY_MS = 30_000`, reset after a successful connect). `react/useTable.ts` re-subscribes on the new connection and reports `isReady === false` until the subscription is applied. It does not reconnect when the app itself asked to disconnect (`isDisconnectRequested`). `DbConnection` is now marked inactive before `disconnect`/`connectError` callbacks fire. Documented in `docs/docs/00200-core-concepts/00600-clients/00700-typescript-reference.md`: "Reconnect attempts use exponential backoff, starting at 1 second and doubling after each consecutive failure up to a 30 second maximum".
- Before this range: at v2.5.0 `connection_manager.ts` had no reconnect logic; after a drop the provider stayed `isActive: false` with stale state until remount/reload.
- Headline idea: "Wi-Fi drops. App recovers."
- One-sentence description: If a React app loses its connection to SpacetimeDB, it now quietly reconnects on its own and picks its live data back up — no page refresh needed.
- Visual idea: a React UI mock (chat or leaderboard) with a status pill "Connected"; a Wi-Fi icon cuts → pill turns red "Reconnecting… 1s", "2s", "4s" (backoff ticks on a small timeline) → Wi-Fi back → pill green, table rows refill and a new message streams in. Optional before/after: "before" frame stays red with a "refresh the page" hint.
- Caveats (don't claim):
  - Only for apps using the React `SpacetimeDBProvider` (connection manager). Apps that build a `DbConnection` directly must still handle reconnection themselves (docs say so explicitly). Vue, Svelte and Angular integrations don't use this manager at v2.6.0.
  - SolidJS's provider also uses `ConnectionManager.retain/release`, so it likely benefits, but that is untested and undocumented — don't claim Solid.
  - Don't say "Still you" / same identity unconditionally: the reconnect reuses the same connection builder, so identity is preserved only if the builder carries a token (e.g. `.withToken(...)`); an anonymous connection without a token may come back as a new identity.
  - Don't claim offline writes are queued/replayed; there's no offline mode.
  - It's framed as a fix ("Fix TypeScript React provider reconnect"); the release notes don't mention it at all.
- Sources: PR #5185 (commit e415c924a9); `v2.6.0:crates/bindings-typescript/src/sdk/connection_manager.ts` lines ~49-60 and `#scheduleReconnect`; docs diff in the same commit.

### C++ modules get the typed query builder — HIGH (MEDIUM if the audience isn't C++/Unreal)
- What changed (exact, verified): #4664 added `crates/bindings-cpp/include/spacetimedb/query_builder.h` + `query_builder/{table,expr,join}.h`. C++ views can now return `Query<Row>` built with `ctx.from[table]`, `.where(...)` / `.filter(...)`, `.left_semijoin(...)` / `.right_semijoin(...)`, comparisons `.eq/.ne/.lt/.lte/.gt/.gte`, and `and_/or_/not_`. Mistakes are compile errors (e.g. `static_assert`: "where() predicates must accept only table columns. Indexed columns are only available in semijoin predicates."; compile-fail tests for incompatible types, non-index joins, invalid join predicates). Query-builder views inherit the underlying table's primary key, so C++ query views get client update events (test module `modules/sdk-test-view-pk-cpp`). Also `Filter::Sql(query)` accepts a typed query for client-visibility filters.
  Example (from `v2.6.0:crates/bindings-cpp/tests/query-builder-compile/pass_query_integration.cpp`):
  ```cpp
  SPACETIMEDB_VIEW(Query<User>, online_member_users, Public, AnonymousViewContext ctx) {
      return ctx.from[user_membership].right_semijoin(
          ctx.from[user],
          [](const auto& memberships, const auto& users) {
              return memberships.user_identity.eq(users.identity);
          })
          .where([](const auto& users) { return users.online; });
  }
  ```
- Before this range: no `Query<` / query builder in `v2.5.0:crates/bindings-cpp/include`; C++ views could only return materialized rows (`std::vector` / `std::optional`). Rust, TypeScript and C# already had query builders at v2.5.0 (`crates/bindings/src/lib.rs`, `crates/bindings-typescript/src/server/views.ts`, `crates/bindings-csharp/BSATN.Runtime/QueryBuilder.cs`), so this is C++ catching up, not a new SpacetimeDB concept.
- Headline idea: "C++ joins the query builder."
- One-sentence description: C++ modules can now write views as type-checked queries — filters and joins that the compiler verifies — instead of hand-assembling result lists.
- Visual idea: a C++ editor; typing `ctx.from[user].where([](auto& u){ return u.online; })` with autocomplete; a deliberately wrong join (`membership.name.eq(user.id)`) gets a red compile error squiggle; fix → green build → a client list of "online users" updates live.
- Caveats (don't claim):
  - Not documented in the docs at v2.6.0 (no C++ query-builder page); don't show a docs page.
  - C++ **procedural** views still can't declare primary keys (v2.6.0 note: "C++ support for primary keys in views will be added in a future release"); only query views inherit PKs from their table.
  - Client-visibility filters (RLS) remain unstable/unenforced — don't advertise the `Filter::Sql(query)` part.
  - This is server-module C++ (the `bindings-cpp` module library), not the Unreal client SDK.
  - Not mentioned in the v2.6.0 release notes.
- Sources: PR #4664 (commit 346e2b2514); `git diff --stat v2.5.0 v2.6.0 -- crates/bindings-cpp`; `v2.6.0:modules/sdk-test-view-pk-cpp/src/lib.cpp`; `v2.6.0:crates/bindings-cpp/include/spacetimedb/query_builder/table.h`.

### C# tables can use Timestamp as a primary key — MEDIUM (LOW as a standalone scene)
- What changed (exact, verified): #5262 added `"SpacetimeDB.Timestamp"` to `ColumnTypeValidation.IsEquatable` in `crates/bindings-csharp/Codegen/Module.cs`, so `[SpacetimeDB.PrimaryKey] public Timestamp CreatedAt;` (and other equatable-only attributes like `[Unique]`) now compiles. Unit test `TimestampPrimaryKeyTable` in `Codegen.Tests/Tests.cs`.
- Before this range: C# codegen rejected Timestamp for these attributes, while Rust/TS already allowed it ("Makes C# consistent with the other module languages").
- Headline idea: "Timestamp keys, now in C#."
- One-sentence description: C# modules can now key a table by a timestamp — handy for logs and time-series — just like Rust and TypeScript modules.
- Visual idea: C# struct with `[SpacetimeDB.PrimaryKey] public Timestamp CreatedAt;` — before: red analyzer error; after: builds.
- Caveats: parity fix only; don't imply timestamp keys are new to SpacetimeDB.
- Sources: PR #5262 (commit 945b2556a5); v2.6.0 note "Timestamps can now be used as primary keys in C# modules".

### Dropped event tables no longer block restarts (and bricked DBs self-heal) — LOW (fix for a 2.5.0 regression)
- What changed (exact, verified):
  - #5288 (also shipped alone as `v2.5.0-hotfix1`): commitlog replay skips the layout refresh for an event table dropped earlier in the same transaction, and `drop_table` now deletes the table's `st_event_table` row.
  - #5289: on database open, `fixup_delete_orphaned_st_event_table_rows` removes orphaned `st_event_table` rows left by earlier versions — "This heals affected databases on their next restart, with no migration or operator action required."
- Before this range: bug **introduced in v2.5.0** by #5269: after an automigration that removed an event table, the database failed to start on every restart with `Failed to open database: DatastoreError: Error deleting row ... from table "st_column" during transaction N playback: ... TableError: Table with ID `N` not found in `st_table`.` ("Observed in production on 2.5.0"). The commitlog itself was intact: "databases bricked by this bug recover with no data loss once opened with this fix".
- Headline idea: n/a for promo (at most a list line: "Restart safety fix for event tables").
- One-sentence description: Fixes a 2.5.0 bug where removing an event table could stop a database from restarting; affected databases recover automatically, with their data intact.
- Visual idea: none recommended (advertising a regression). If needed: a list bullet in a "fixes" card.
- Caveats (don't claim): it's news only to 2.5.0 users; don't imply older versions were affected (the bug came from #5269 in 2.5.0). Don't claim #5289 was in hotfix1 (hotfix1 contains only #5288).
- Sources: PRs #5288, #5289; `git log v2.5.0..v2.5.0-hotfix1`.

### Scheduled functions keep running after a host panic — LOW
- What changed (exact, verified): #5280 (`crates/core/src/host/scheduler.rs`): scheduled reducer/procedure calls are wrapped in `catch_unwind`; on a panic the scheduler logs `scheduled function panicked`, drops that queue item (not rescheduled), and keeps running. The module host is still poisoned as before (`defer_on_unwind`).
- Before this range: "a panic from a scheduled JS reducer/procedure could unwind out of `SchedulerActor::handle_queued`" and crash the scheduler.
- Headline idea: "One crash won't stop the clock."
- One-sentence description: A crash inside one scheduled job no longer takes down the scheduler that runs all your other timers.
- Visual idea: a row of clock icons ticking; one explodes; the others keep ticking.
- Caveats (don't claim): this is about host-level panics (PR mentions JS modules), not ordinary errors thrown by user code, which were already handled; the panicking item is dropped, not retried. Exact pre-fix blast radius (all schedules for the DB vs. a crash of that task) not measured — keep wording vague.
- Sources: PR #5280 (commit fd1447d1d3).

### `spacetime subscribe -n N` tells you when it fell short — LOW
- What changed (exact, verified): #5278 (`crates/cli/src/subcommands/subscribe.rs`): with `-n/--num-updates N`, the command now fails if the connection closes before N updates (`subscription closed after receiving {received}/{expected} updates`) or if `--timeout` expires first (`subscription timed out after {timeout_secs}s after receiving {received}/{expected} updates`). Help text: "Timing out before receiving `-n` updates is an error."
- Before this range: a server-side close before N updates exited successfully (silent).
- Headline idea: n/a (list item: "Scripts catch missing updates").
- One-sentence description: Scripts using `spacetime subscribe -n` now get a clear error when fewer updates than expected arrive.
- Caveats: `spacetime subscribe` is still labelled "UNSTABLE" in the CLI reference; motivated by smoketests. Behavior change could break scripts that relied on the old lenient exit.
- Sources: PR #5278; CLI reference diff `docs/docs/00300-resources/00200-reference/00100-cli-reference/00100-cli-reference.md`.

## Left out (not user-visible or not promo-worthy)
- #5287 Parameterized query plans, #5275 Unify index key representation in query plan, #5263 Stop tracking multiple plan types in the subscription cache — internal query-engine refactors preparing for parameterized views ("parameterized plans are still not shared across subscriptions yet"). No user-facing behavior or measured speedup to claim.
- #5327 Update docs for view primary keys — docs-only; the feature itself shipped in 2.4.1/2.5.0.
- #5243 docs: improve docs and agent discovery metadata — docs-only (TS view examples fixed; `/docs/robots.txt`, `llms.txt` alternates, `/docs/.well-known/agent-skills/` generated). Could be an "AI-agent friendly docs" footnote but it's website plumbing.
- #5265 Document schedule table lifecycle, #4742 Fix broken links in auth docs — docs-only.
- #5176 Cross compile CLI binaries for ARM — CI-only (aarch64 Linux binaries existed before; now built on `ubuntu-22.04` with a cross toolchain).
- #5255 Compilation guard && `tokio::sync` re-export from `runtime` crate — internal build hygiene.
- #5297 Disable automatic snapshots in replay tests, #5298 Move index scan tests into benchmark job — test infra.
- #5299, #5313, #5316, #5329 CLA gate workflow changes; #5314 Announce GitHub releases in Discord — CI/ops.
- #5326 Version bump `2.6.0` — release mechanics.
- Items in the v2.6.0 notes that are not in this range: #5111 (v2.4.1), #5246 and #5269 (v2.5.0), #5074 and #5077 (v2.3.0), #5322 (never merged).

## Open doubts
- The 2.6 range is thin. Only two items can carry a scene (React auto-reconnect, C++ query builder); everything else is a small parity fix or bug fix. The release-note headline (view primary keys) belongs to the 2.5 video.
- Commitlog knobs (#5074: `max_segment_size`, `write_buffer_size`, `preallocate_segments` in `config.toml`, default write buffer 8 KiB → 128 KiB) are real but shipped in v2.3.0; exclude from both videos unless a v2.3 video is planned.
- React reconnect: confirm with the SDK team whether SolidJS/TanStack users also get it (Solid's provider uses the same `ConnectionManager`).
- C++ query builder is undocumented at the tag; check the live docs before showing any doc URL.
- Whether Maincloud ran `v2.5.0-hotfix1` (the tag has no GitHub release) — irrelevant for the video, but explains why the event-table fix appears twice.
