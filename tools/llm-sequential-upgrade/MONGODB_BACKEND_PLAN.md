# MongoDB Backend — Implementation Plan

Adding `mongodb` as a third backend to the **cost-to-done** sequential upgrade
benchmark, at full methodological parity with the existing SpacetimeDB-vs-PostgreSQL
run. These are official, investor-facing tests, so the bar is *fairness parity*,
not just "it runs."

## Decisions locked

| Decision | Choice | Rationale |
|---|---|---|
| Mongo stack | **Express + Mongoose + Socket.io** (standard MERN) | What "standard MongoDB" means to most devs; structurally symmetric to the Postgres column (Express + Drizzle + Socket.io), so SpacetimeDB-vs-Mongo measures the same axis SpacetimeDB-vs-Postgres did. |
| Real-time | **Manual Socket.io broadcast** (NOT change streams) | Change streams would need a replica set and hand Mongo a real-time-ergonomics advantage Postgres didn't get. Documented as a caveat for transparency. |
| Framing | **Pairwise: SpacetimeDB vs Mongo** | Its own head-to-head, like Postgres. Minimal report/viewer changes (reuse 2-column layout). Run Mongo in its own comparison dir, not alongside Postgres. |
| Scope (now) | **Cost-to-done harness only** (run.sh + manual grading) | Metrics generator + public viewer deferred. |
| Grading | **Manual** (human, in-browser) | No dependency on the automated Playwright suite for the comparison numbers. |
| Parity target | **Postgres** treatment, not SpacetimeDB's | Mongo is standard Node/TS; it needs Postgres-level scaffolding, not STDB's extra sdk-rules/templates. |

Grading artifacts (`BUG_REPORT.md`, `ITERATION_LOG.md`) follow the published formats —
templates live in [`templates/`](templates/).

## Out of scope (deferred)

- Perf throughput benchmark (`perf-benchmark/` — needs a third client + `'mongo'` union arm).
- `METRICS_DATA.json` generator + public viewer Mongo rendering — **required before
  anything reaches the public results page**, but not part of this effort.

---

## Phase 0 — Grading approach (manual)

Grading is done **manually** by a human in the browser, scoring each feature against
the feature spec — the same way the published SpacetimeDB/Postgres runs were graded.
The grader files a `BUG_REPORT.md` (see [`templates/`](templates/)) into the app dir;
`./run.sh --fix <app-dir>` reads it; repeat until all features pass.

**No hard dependency on the automated Playwright suite** for the comparison numbers,
so the uncommitted `test-plans/playwright/` directory is *not* a blocker for this effort.

Manual-grading parity checklist for Mongo:
- App reachable on its own Vite port (`6373`) with title **"MongoDB Chat"** (visual
  disambiguation from the other backends during testing).
- `BUG_REPORT.md` / `ITERATION_LOG.md` produced in the canonical format (templates in
  [`templates/`](templates/)) so artifacts match the published Postgres/SpacetimeDB ones.
- Clean DB state between feature tests via the new `reset-app.sh` Mongo arm (Phase 4).

> Optional, not blocking: if you later want automated grading too, retrieve and commit
> `test-plans/` and audit specs for literal title assertions (`"PostgreSQL Chat"` etc.)
> that would need a `"MongoDB Chat"` variant.

---

## Phase 1 — Infrastructure ✅ DONE (verified)

Mongo service added to `docker-compose.otel.yaml`; container
`llm-sequential-upgrade-mongodb-1` comes up healthy on `6437`, pre-flight ping
returns `{ ok: 1 }`.

**`docker-compose.otel.yaml`** — add a single-node Mongo service (no replica set needed):
```yaml
  mongodb:
    image: mongo:7
    ports: ["6437:27017"]
    volumes:
      - llm-sequential-upgrade-mongodata:/data/db
    healthcheck:
      test: ["CMD","mongosh","--eval","db.runCommand({ping:1})"]
      interval: 5s
      timeout: 5s
      retries: 5
```
Add `llm-sequential-upgrade-mongodata` to the `volumes:` block.
Container resolves to `llm-sequential-upgrade-mongodb-1`.

---

## Phase 2 — Prompt / spec files (clone from Postgres) ✅ DONE

All three files created: `backends/mongodb.md`,
`../llm-oneshot/apps/chat-app/prompts/language/typescript-mongodb.md`,
`../llm-oneshot/apps/chat-app/prompts/base_mongodb.md`.

Feature specs, grading rubric, and composed/feature prompts are reused **unchanged**
(confirmed backend-agnostic). Three new files:

**2a. `../llm-oneshot/apps/chat-app/prompts/language/typescript-mongodb.md`**
(clone `typescript-postgres.md`):
- Title `"MongoDB Chat"`
- MongoDB brand palette (only place colors live): green `#00ED64`, slate `#001E2B` / `#023430`, full 12-value set matching Postgres structure
- Architecture: `Node.js + Express + Mongoose + Socket.io`
- Project path / DB label: `.../mongodb/...`, database `chat-app`

**2b. `backends/mongodb.md`** (clone `backends/postgres.md`, ~315 lines):
- Connection table: `localhost` / port `6437` / container `llm-sequential-upgrade-mongodb-1` / URL `mongodb://localhost:6437/chat-app`
- Pre-flight: `docker exec <container> mongosh --eval "db.runCommand({ping:1})"`
- Phase 1 (server): Mongoose deps; `src/models.ts` (Mongoose schemas) replaces Drizzle `schema.ts`; **delete the `drizzle-kit push` step** (Mongo is schemaless)
- Phase 3 (client): reuse Postgres nearly verbatim, but **Vite port 6373** (not 6273); keep the `io()`-without-hardcoded-URL proxy warning
- App Identity: `<title>` + visible header MUST be `"MongoDB Chat"`
- Port table: Mongo `6437` / Express `6001` / Vite `6373`
- "Key Differences from SpacetimeDB" table: rewrite right column for Mongo
- **Caveat note:** manual Socket.io chosen over change streams for symmetry

**2c. `../llm-oneshot/apps/chat-app/prompts/base_mongodb.md`** (clone `base_postgres.md`;
one-shot entry point, for completeness).

---

## Phase 3 — Generalize `run.sh` (2-backend → N-backend) — CORE WORK ✅ DONE (plumbing verified)

All 7 sites edited; `bash -n` clean. `detect_backend` unit-tested (marker correctly
disambiguates mongo vs postgres — both have `server/`; legacy apps fall back to
dir-shape) and the Mongo pre-flight passes against the live container.

**Smoke-test gate: ✅ GREEN.** `./run.sh --level 1 --backend mongodb` succeeded
(`mongodb/results/chat-app-20260615-161246`): exit 0, `DEPLOY_COMPLETE`, MERN stack
(Mongoose + Express + Socket.io, no Drizzle / no change streams), `<title>MongoDB Chat`,
marker + `metadata.backend=mongodb`, frozen inputs include the mongo prompt files,
zero build reprompts, $1.50 / 33k tokens with COST_REPORT.md in the standard format.

`run.sh` is hardwired `spacetime`-vs-`else(=postgres)`. A Mongo app has a `server/`
dir like Postgres, so it is **misdetected as Postgres in ~7 spots**.

| # | Location | Change |
|---|---|---|
| 1 | `:28` | Add `MONGO_CONTAINER="${MONGO_CONTAINER:-llm-sequential-upgrade-mongodb-1}"` |
| 2 | `:72–82` | Add `VITE_PORT_MONGO=$((6373+RUN_INDEX))`; convert `if spacetime…else` selector to explicit 3-way (non-spacetime must stop meaning Postgres) |
| 3 | `:162–192` | Add `elif mongodb` pre-flight: mongo ping + per-run DB isolation (`chat-app_runN`, mirroring Postgres `spacetime_runN`) + `MONGO_CONNECTION_URL` |
| 4 | `:277–281, 468–474, 536–542` | **Root-cause fix:** at generate time write `echo "$BACKEND" > "$APP_DIR/.benchmark-backend"`; all three detection sites read the marker first, fall back to directory-shape. Permanently solves the Mongo/Postgres `server/` collision |
| 5 | `:421–425` | `snapshot_inputs`: generic `cp backends/$BACKEND.md` already works; no Mongo template files (match Postgres lean treatment) |
| 6 | `:680–723` | CLAUDE.md assembly: `guided` else is already generic ✓; add `mongodb` arm to `minimal`/`standard` blocks (they hardcode Postgres text) |
| 7 | `:730–740` | Parallel-run port `sed` patching: add `6373` + `mongodb://…` rewrites |

**Gate:** `./run.sh --level 1 --backend mongodb` generates/builds/deploys on `:6373`,
no Postgres-path leakage, `metadata.json` shows `"backend":"mongodb"`, `--fix`/`--upgrade`
re-detect mongo via the marker.

---

## Phase 4 — Grading harness (manual) ✅ DONE (verified)

Required changes complete. The automated-grading plumbing (`grade-playwright.sh` /
`grade-agents.sh`) is left for later (optional).

| File | Change | Status |
|---|---|---|
| `reset-app.sh` | Marker-based detection + new `mongodb` arm: `mongosh <db> --eval "db.dropDatabase()"`; added `MONGO_CONTAINER`. **Live-tested** (seeded 1 doc → reset → 0). | ✅ |
| `templates/BUG_REPORT.template.md`, `templates/ITERATION_LOG.template.md` | Canonical formats matching the published results | ✅ |
| `grade.sh` | Marker-based detection + port resolved from `metadata.json vitePort` (fallbacks aligned to run.sh: 6173/6273/6373). **Verified** mongo app → backend `mongodb`, port `6373`. | ✅ |
| `GRADING.md`, `GRADING_WORKFLOW.md` | Added MongoDB URL/port rows; also corrected the stale 5173/5274 ports to the real 6173/6273/6373 scheme. | ✅ |
| `grade-agents.sh`, `grade-playwright.sh` | `mongodb` arm | Deferred (automated grading only) |

---

## Phase 5 — Reporting (pairwise → minimal) ✅ DONE

- **LOC counter** (`generate-report.mjs`): already covers Mongo — its `server/` branch
  counts any Express backend (postgres + mongodb), `client/src` is generic. Renamed the
  comment to make this intentional. `node --check` clean.
- **`benchmark.sh`:** accepts `--backend mongodb` (no hardcoded backend validation), so
  pairwise batches work as-is. For a 2-up batch, run `--backend spacetime` and
  `--backend mongodb` separately, or set `BACKENDS=("spacetime" "mongodb")`.
- Pairwise framing keeps the existing 2-column comparison table valid (run Mongo in its
  own comparison dir vs SpacetimeDB).

---

## Phase 6 — Validation & acceptance

1. **Smoke:** `./run.sh --level 1 --backend mongodb` → live on `:6373`, `DEPLOY_COMPLETE`,
   `COST_REPORT.md` with non-zero `cost_usd`.
2. **Detection:** `--fix` and `--upgrade` on the L1 app re-detect `mongodb`, snapshot `level-N`.
3. **Grading:** grader on `:6373` scores all features on the identical rubric;
   `reset-app.sh` cleanly wipes the Mongo DB.
4. **Fairness audit (investor-critical):** diff the frozen `inputs/` snapshot Mongo-vs-Postgres
   — confirm identical composed prompt, identical UI-contract stripping, same CLAUDE.md
   structure, unique run-id cache-bust present, same `--rules guided`. Any asymmetry = methodology bug.
5. **Full run:** L1→L12 sequential upgrade + grade/fix loop, mirroring the canonical procedure.

---

## Sequencing

```
Phase 1 (Mongo container) ──┐
Phase 2 (3 prompt files)  ──┤  parallel, cheap, no blockers
        │
Phase 3 (run.sh N-backend)     ← core; gate on L1 smoke
        │
Phase 4 (reset-app Mongo arm + grade docs/templates)
        │
Phase 5 (LOC counter + batch)
        │
Phase 6 (validate parity → full L1→L12, manual grading)
```

(Manual grading removes the old `test-plans/` blocker — see Phase 0.)

## Open risks

- **`server/` detection collision** is the most error-prone change; the
  `.benchmark-backend` marker (Phase 3 #4) is the clean fix — treat as mandatory.
- **Change-streams caveat** must be documented in `backends/mongodb.md` (like the
  perf README documents Postgres's rate-limit caveat) for transparency.
- **Deferred last mile:** `METRICS_DATA.json` generator + viewer's hardcoded 2-series
  rendering — nothing reaches the public results page without these.

## Files touched (summary)

New:
- `backends/mongodb.md`
- `../llm-oneshot/apps/chat-app/prompts/language/typescript-mongodb.md`
- `../llm-oneshot/apps/chat-app/prompts/base_mongodb.md`
- `templates/BUG_REPORT.template.md`, `templates/ITERATION_LOG.template.md`, `templates/README.md` — **done**

Edited:
- `docker-compose.otel.yaml`
- `run.sh` (7 sites)
- `reset-app.sh` (new Mongo reset arm) — required
- `grade.sh` — required; `grade-agents.sh`, `grade-playwright.sh` — optional (automated grading only)
- `generate-report.mjs` (LOC counter)
- `benchmark.sh` (backend list)
- `GRADING.md`, `GRADING_WORKFLOW.md` (docs)

Retrieve & audit (blocker):
- `test-plans/` (+ `test-plans/playwright/`)
