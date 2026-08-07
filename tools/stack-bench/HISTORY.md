# What this benchmark has learned

Newest first. One entry per finding: what happened, what it means, what changed.
Numbers here are n=1 unless stated. Anything voided stays listed — a retracted
result is a finding.

Other files: `FINDINGS.md` (product bugs with repros), `STDB-FRICTION.md`
(per-run friction, auto-appended), `ROADMAP.md` (what is still to build).

---

## 2026-08-07 — Documenting one command was worth more than any code change

`spacetime dev` (watch + auto rebuild/publish/regenerate, and it streams module
logs) existed the whole time and the backend docs never mentioned it. They
prescribed the manual publish → generate loop instead.

| ecommerce L1, spacetime | manual loop | `dev` documented |
|---|---:|---:|
| unaided score | 40/48 | **48/48** |
| fix rounds | 3 | **0** |
| total cost | $22.51 | **$10.29** |
| duration | 127 min | **32 min** |
| file inspections | 240 | 102 |

Every SpacetimeDB cost figure recorded before this date measured a workflow the
product does not require. **Check documentation parity before believing any
backend is expensive.** Caveat: n=1 vs n=1, and denominators differ (48 vs 49)
because contention criteria move in and out of INCONCLUSIVE — the cost, time
and zero-fix-rounds are the solid part, the score jump is directional.

## 2026-08-07 — Capability parity audit

Prompted by the above. Rule it produced: **when the prescribed stack grants a
capability implicitly, the equivalent must be named explicitly for the others,
or the benchmark measures documentation instead of databases.** `tsx` gave
postgres and mongo hot-reload for free; SpacetimeDB's equivalent went unsaid.

Audit found the reverse asymmetry too: neither postgres nor mongo had any
data-inspection guidance, so they queried their databases 4 and 2 times while
spacetime queried 23 after finding `spacetime sql` unaided. All three packs now
state the same four things (code goes live, server output, inspect data, data
disposable), verified by grep.

## 2026-08-07 — Where the fix-loop money goes

Fix rounds have poor marginal returns and the loop pays for its own noise.

```
spacetime  40/48 → 45/49 → 46/48 → 47/49   ($15.58 for +7)
postgres   39/50 → 48/50 → 48/50           ($11.15 for +9, round 2 gained nothing)
mongodb    47/50 → 48/50 → 48/50           ($7.07 for +1)
```

Round 1 does the work; later rounds mostly confirm a plateau. Also found:
INCONCLUSIVE criteria were being written into `BUG_REPORT.md` as defects, so a
run spent $6.83 across two rounds chasing two failures that did not exist —
more than the $5.75 build. Fixed (report-bugs filters inconclusive). The bias
was not neutral: contention criteria go inconclusive most on the backend whose
writes the replay mechanism cannot capture.

## 2026-08-07 — Systems criteria: all three backends passed

First run of the systems suite (out-of-band writes, deploy window,
enumeration-during-mutation). 901a/b/c and 902a passed on **all three**
backends — postgres via socket.io reconnect-refetch, mongo via polling,
spacetime by construction. **Not promotable**: a criterion earns points only by
failing a real build for a real reason. They remain 0 points and are reported
as a diagnostic with a bill attached — same outcome, three very different
implementation costs.

## 2026-08-07 — Module runtime has no crypto

Postgres and mongo builds wrote `import bcrypt from 'bcryptjs'`. SpacetimeDB
builds, across two runs, produced: a hand-written SHA-256 (with a comment
explaining there is no system crypto), a non-cryptographic hash, and **plaintext
password storage**. The benchmark scored the plaintext app 41/48 without
noticing — a hole in the rubric as much as a gap in the product. Recorded in
`STDB-FRICTION.md`; no password criterion added by decision.

## 2026-08-07 — Cost inverts depending on what you measure

One-shot build cost, ecommerce L1: spacetime **$6.93** / postgres $8.39 /
mongodb $12.42, at 109 / 136 / 191 turns. SpacetimeDB is consistently the
cheapest to *build* and was the most expensive to *iterate on* — which the
`dev` finding then explained. Code size held across every run: ~464–681 server
LOC and 3 runtime deps against 868–989 LOC and 10–13 deps.

Totals swing hard run to run ($5.97 → $22.51 → $10.29 for the same task), so
**nothing is quotable at n=1.** Repeat trials outrank new criteria.

## 2026-08-06 — Every score before this date was void

Transcript audits found generated apps reading the harness that grades them:
scenario files, `grade.mjs`, contracts, and the benchmark's own notes — up to
44 reads in one run. Contamination was asymmetric *against* SpacetimeDB.

Causes and fixes:
- `--dangerously-skip-permissions` is bypassPermissions and disables
  `permissions.deny` entirely. A deny list under it is decorative. Fixed by
  `--permission-mode acceptEdits` (+ `allow: [Bash]`, since a build that cannot
  run `npm install` does not build).
- The prompt printed the linter's absolute path — a signpost two directory
  listings from the marking scheme. The linter now answers over loopback and
  the shim names a port.
- Apps were built inside `results/`, underneath the harness. They now build in
  isolated, stamped work directories.

Controls that came out of it, all still in force: `probe-sandbox.mjs` gates
every run, `leak-audit.mjs` audits every run's transcripts, transcripts are
archived against the CLI's 30-day prune, and an audit that fails to run is
recorded as *not usable* rather than "unknown".

## 2026-08-06 — Oracles lie in both directions

Every one of these looked fine until run against a known answer:
- The first sandbox probe reported PASS by matching the word "denied" **inside
  the file it had just read**.
- `leak-audit` counted blocked attempts as leaks, which would have voided
  exactly the runs the sandbox was protecting.
- It also took the most-common cwd as the boundary, so a SpacetimeDB build's
  reads of its **own** generated bindings registered as escapes.
- A run was voided over a memory file that was the session's **own** auto-memory
  — a diary, not a leak.
- `clickConcurrently` swallowed failed clicks and nearly published a fake
  "lost update"; INCONCLUSIVE was once scored as failure, penalising
  SpacetimeDB for WebSocket writes.

Standing heuristic, 5/5 so far: **when all backends fail a criterion
identically, suspect the oracle, not the apps.** Hence mutation testing before
any criterion scores.

## 2026-08-06 — The June and April sequential runs are unverifiable

Not clean — unverifiable. Their transcripts were pruned by the CLI's 30-day
retention before anyone archived them, and OTel telemetry records `tool_name`
but never arguments, so it can prove a read happened and never what was read.
Both eras ran the same permission configuration later proved to provide no
protection. Retention is now pinned and every run archives its transcripts.

## Harness self-inflicted failures worth remembering

- Reused work directories: one stale handle wedged every future run on that
  backend. Fixed with stamped per-run directories.
- `pidsMatching` matched the searching process's own command line; killTree
  then took down the run — and, once, a browser that had nothing to do with the
  benchmark. Now excludes its own ancestry.
- Rollback deleted `app/server` while the dev server watched it → EBUSY killed
  a finished run *after* grading. Servers now stop before rollback.
- The lint server first listened inside the agent process, which is blocked in
  `execFileSync` for the whole session; `curl` hung until the build's own
  timeout and one run shipped with 14 missing hooks having never seen a lint
  result.
- Grading detail lived only in the work dir the run then deleted, so "why did
  this criterion fail" was unanswerable. Now copied to `results/<run>/grading`.
- `reset-db.sh` fed the app's `ws://` URI to a CLI that only speaks http,
  aborting a $10 grading pass.
