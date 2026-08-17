# What this benchmark has learned

Newest first. One entry per finding: what happened, what it means, what changed.
Numbers here are n=1 unless stated. Anything voided stays listed — a retracted
result is a finding.

Other files: `FINDINGS.md` (product bugs with repros), `STDB-FRICTION.md`
(per-run friction, auto-appended), `ROADMAP.md` (what is still to build).

---

## 2026-08-12 — paid runs now prove the machine is ready first

The harness used to discover missing images, stopped databases, wrong ports,
bad credentials, or inadequate Docker resources only after the benchmark had
started. A new exact-scope `preflight` checks the requested stacks, levels and
selection plus the complete supported environment before a model is called.

The admission gate verifies Docker/Compose, architecture, CPU, memory, host and
container disk, clock skew, image/platform identity, digest-pinned healthy
database services, free ports, provider-declared credentials and outbound
destinations, the repository Linux CLI, persistent result mounts, and inherited
run ownership state. Its smoke starts the real build image but no model. Every
real run executes it automatically and retains a typed child artifact.

On its first exact three-stack L1–L2 run, preflight correctly found both database
containers stopped. After the stated Compose remediation, all 24 checks passed,
including two outbound destinations and a container-to-host result write. All
194 tests, the deterministic build/repair loop, lifecycle fault injection, and
the real SpacetimeDB Docker publish/query/log-stream smoke pass.

---

## 2026-08-12 — one status table now decides every benchmark verdict

Typed evidence already prevented prose from being the protocol, but each
consumer still independently mapped the four states into concepts such as
measured, repairable, application failure, and run outcome. That duplication
could drift as new reports or controls were added.

One immutable disposition table now owns those semantics and the operator label
for every state. Scoring, run outcomes, null and mutation controls, comparisons,
reference qualification, nested actions, repair selection, and console output
all consume it. Diagnostic humanisation and redaction remain presentation-only.
Adversarial tests prove that prose claiming the opposite verdict cannot change
classification, including through the actual bug-report command.

All 190 tests, both deterministic loop paths, injected teardown faults, static
definition/composition/calibration/reference validation, and the real Docker
publish/query/log-stream smoke pass after the change.

---

## 2026-08-12 — coding agents became versioned inputs, not executable paths

The runner previously accepted any `--agent` file and trusted its last stdout
line as a session result. That made the executable path the effective interface:
supported modes, timeouts, model defaults, authentication, usage fields, and
result identity were implicit and malformed output could survive until later
aggregation.

Agent adapter schema v1 now statically registers the current Claude Code
implementation, deterministic loop fixture, lifecycle fault injector, and
model-free reference deployer. Each adapter declares its supported modes,
deadline, default model, API-key environment name, and content-bound identity.
The engine emits one normalized request, validates the completion identity and
all usage/cost/timing fields, rejects unknown output, and stores an explicit
transcript reference. API keys now pass from the controller to the adapter via
its declared environment variable instead of a controller child-process
argument. The arbitrary-path switch was removed.

The full 186-test suite, deterministic build/repair loop, and injected non-zero
adapter failure cleanup pass. This is an engine boundary, not a claim that a
second live provider has been qualified; adding one still requires its own
container implementation, credential handling, provenance, and qualification.

---

## 2026-08-12 — pre-v1 artifacts became an archive, not an input format

An inventory found 678 likely retained result artifacts, all produced before the
current schema. Keeping a runtime normalizer would have guessed identities and
verdict semantics that those runs never recorded. The raw result tree is now
preserved unchanged under `archive/pre-v1/results/`, bound by a tracked manifest
and deterministic SHA-256, and excluded from qualification and comparison.

The active reader now accepts only artifact schema v2. Grade criteria require
typed evidence; the duplicate `passed`, `inconclusive`, and `detail` projections
are gone; comparisons require recipe and selection identities with no override.
Historical reference reports remain opaque provenance links, while current
Docker qualification is required before a reference can support a decision.

## 2026-08-12 — backend behavior moved behind one complete adapter contract

The engine used to select backends repeatedly across orchestration, grading,
reset, build isolation, reference deployment, reporting, and teardown. Those
decisions now live in statically registered, versioned capability providers for
SpacetimeDB, PostgreSQL, MongoDB, and the deterministic offline stack. The
engine-facing paths ask for capabilities; they do not dispatch on concrete
backend names.

Registry construction now rejects incomplete adapters, missing required
operations, mismatched provider identities, unknown capabilities/operations,
duplicates, and malformed versions before resources or paid sessions begin. A
complete fake stack registers without engine changes. Moving build isolation
behind the contract exposed a real leak: PostgreSQL and MongoDB coding
containers had been receiving the persistent SpacetimeDB CLI configuration.
They now receive neither that credential-bearing mount nor SpacetimeDB SDK/CLI
artifacts. Docker smoke, injected teardown faults, both deterministic loop
paths, all static validators, and all 182 tests pass after the change.

## 2026-08-12 — prose stopped deciding benchmark outcomes

Criterion meaning was spread across three informal signals: `passed`, an
optional `inconclusive` flag, and strings beginning with `INCONCLUSIVE:` or
`setup failed:`. Different consumers read different subsets. A harness crash
could therefore be flattened into an oracle gap, comparison reparsed wording
that scoring did not, and mutation/null analysis inferred execution phase from
an English sentence.

New grades now emit strict check evidence schema v1 with four distinct states
(`passed`, `failed`, `inconclusive`, and `harness_failure`), one machine code,
phase, actor, observation and expectation, retryability, timing, attachment
references, sensitivity labels, and the complete versioned action evidence
trail. Successful setup evidence is retained instead of discarded. A failed
setup is stored once on the feature; blocked criteria link to it so the report
does not multiply the same action transcript across every criterion.

Artifact validation rejects malformed evidence. Scoring, cross-run comparison,
top-level outcomes, null control,
mutation control, reference qualification, and repair-report selection now use
the typed verdict. Pre-v1 artifacts were subsequently archived and removed from
the active reader rather than assigned meaning they did not originally record. Tests use
deliberately misleading summaries to prove prose can no longer change a new
artifact's classification.

## 2026-08-11 — the first canonical reference passed repeated live grading

The preserved MongoDB fixture now passes the current L1 harness twice from
fresh source copies in Docker: 51/51 on both runs, 48/48 criteria including all
nine zero-point controls, identical criterion fingerprints, the same immutable
image, and complete lease/container/lock teardown. The retained qualification
summary is
`archive/pre-v1/results/reference-live/reference-live-mongodb-20260811163258-1474320.json`
(SHA-256 `d49319ca…5f5`). It remains a candidate until its mutations are
regenerated and cleanly killed.

Getting a trustworthy pass exposed three defects in the measurement path.
Windows reset commands were being handed to Bash, a failed reseed could leave
the old server answering and masquerade as success, and an ungraded benchmark
could exit zero. Reset/restart are now structured Node operations authenticated
by the backend lease, and ungraded/harness-failed runs return non-zero. A later
Docker command timeout was also being blamed on the app; child-process
infrastructure failures now become explicit inconclusive evidence.

The run also found an over-constrained oracle. Replaying a generic cart-add as
another authenticated customer may safely add to that caller's own cart; HTTP
success is not evidence that the owner was modified. Criterion 109a now asserts
the actual ownership boundary instead of demanding rejection from
identity-derived APIs. The corrected oracle passed twice live.

The PostgreSQL candidate then passed the identical gate twice: 51/51, all 48
criteria and nine zero-point controls, stable fingerprints, one immutable image
and complete teardown. Its summary is
`archive/pre-v1/results/reference-live/reference-live-postgres-20260811165753-1480356.json`
(SHA-256 `b90c03ea…1ef0`). This matters because it is the preserved L2 source,
selected after the older L1 source failed a zero-point control; the live result
now proves rather than assumes that it is a valid current L1 reference.

SpacetimeDB initially failed before and during grading because three control
paths still assumed host execution: reset invoked the host CLI against a
container-only dependency, while action discovery and direct SQL scraped
literal module/URI values from generated client config. All now use the
authenticated lease and the pinned CLI inside the exact leased build container.
The qualifier also uses dedicated loopback port `3310`, refusing rather than
adopting the unrelated process found on `3210`, and final teardown records the
lease as released.

After those fixes SpacetimeDB passed twice at 51/51, all 48 criteria and nine
zero-point controls, with stable fingerprints and complete host/container/lock
cleanup. Summary:
`archive/pre-v1/results/reference-live/reference-live-spacetime-20260811172943-1487716.json`
(SHA-256 `fbe65249…b45a`). All three ecommerce L1 candidates have now cleared
compile and repeated live Docker gates; exact mutation kills remain the
promotion blocker.

## 2026-08-11 — reference apps are now byte-bound and Docker-buildable

Historical full scores were not enough to run the hardened mutation gate: none
of the seven old manifests matched a preserved source tree. A registry now owns
every backend/track/level tuple and legacy manifest, binds both the preserved
origin and sanitized import by SHA-256, and distinguishes blocked, candidate and
active fixtures. Ecommerce L1 has one candidate for MongoDB, PostgreSQL and
SpacetimeDB; chat remains blocked pending reconstruction.

The import audit immediately earned its keep. Large files contained literal
tool-output truncation markers, three locks were invalid JSON, another lock was
semantically incomplete, and both Node servers had type errors obscured by dev
transpilation. The Spacetime path also proved the Linux CLI alone is incomplete
for binding generation: schema extraction launches a sibling
`spacetimedb-standalone`. The repository build now produces and mounts both
binaries, and stages a stable SDK tarball so absolute local SDK dependencies can
still use strict `npm ci`.

The combined model-free Docker gate now passes all three candidates in the
pinned image `sha256:b404d…c10`: locked installs, server/client TypeScript,
Spacetime module build, schema extraction, generated bindings and Vite builds.
Evidence is retained in
`archive/pre-v1/results/reference-builds/reference-build-20260811152751-1399692.json`
(SHA-256 `feb6176d…a0ae`). This is a compile qualification, not promotion:
fixtures remain `candidate` until two identical live L1 grades pass every
criterion and regenerated mutants are cleanly killed.

## 2026-08-11 — mutation success used to include invalid evidence

The mutation runner could exit zero after skipped mutants, setup-only score
drops, collateral feature failures, failed reset/redeploy/readiness checks, or
an already-failing baseline. It compared aggregate feature scores and printed a
warning when setup failed, but still counted that mutant as caught. This made a
green mutation run materially weaker than its name implied.

Mutation control is now criterion-level and fail-closed. A clean kill requires
a fully passing reference baseline, one unique source anchor per edit, a
conclusive failure of every exact `kills` criterion, no setup failure and no
collateral regression. Infrastructure faults produce structured atomic failure
artifacts. Manifest metadata binds backend, track, level and scenario; resets
remain lease-authenticated, hosted apps must restart to reseed, and SpacetimeDB
modules are republished from mutated and then restored source through that
lease rather than a free-form hard-coded publish command.

Binding manifests to real scenario ids immediately exposed three stale entries
in `spacetime-l1.json`: unread, typing and receipt mutants pointed at the old
feature numbers and therefore could not prove their intended current criteria.
Those mappings are corrected. The focused suite now passes 51/51, including
manifest/schema/target checks and adversarial result classification, and a
runner smoke proves harness faults exit 2 with `harness_failure` evidence.

This still does not complete the production gate: checked-in fully passing
reference apps do not exist yet, so none of these manifests has been rerun in
clean CI against the hardened runner. A follow-up inventory tested every
manifest against every preserved source tree for its backend and track: **zero
of seven manifests matched even one source tree**. Their anchors belonged to
transient revisions that are gone. All seven are now explicitly
`legacy-unreproducible`, and the runner refuses anything not marked `active`.
Activation requires a canonical checked-in reference app and unique anchors;
copying an arbitrary historical result can no longer masquerade as recovery.

## 2026-08-11 — the null app scores zero, but setup failure is not mutation proof

The real L1-L2 grader ran every validated chat and ecommerce suite against a
reachable blank HTTP application. All 129 point-bearing criteria / 179 points
failed conclusively; none passed and none became an oracle gap. The null-app
production gate therefore passes and now runs in Linux CI with its evidence
artifact retained.

Due diligence changed the interpretation. Every scored failure came from
feature setup (usually sign-up or sign-in) before the criterion's own steps ran.
That is useful evidence that a nonexistent product cannot score, but it cannot
show that an assertion distinguishes a correct feature from a subtly broken
one. The null analyzer now reports setup-stage and assertion-stage failures
separately, and also accounts for zero-point diagnostics. Reference apps plus a
caught mutation per scored criterion remain mandatory; the null gate does not
launder the 2026-08-10 "never observed failing" audit into oracle validation.

## 2026-08-11 — `spacetime dev` now keeps one identity through publish and logs

The Docker smoke's log-stream warning was a real CLI defect, not a host-alias
or container-networking problem. On a clean config, programmatic publish took
`Config` by value, directly logged in, and saved the new token only in its clone
and on disk. The still-running `dev` process retained a tokenless `Config`, so
starting the log stream directly logged in again as a different identity. That
identity did not own the database and the server correctly rejected its logs.

Programmatic publish now mutably borrows the caller's `Config`. The publisher's
token therefore remains live for the log stream and later rebuilds. The CLI
compiles, all 169 CLI library tests pass, and a newly built Linux CLI passes the
unchanged Docker reproduction with publish, SQL, watcher, lease, immutable
image and `logStreamingAuthorized: true`. The smoke now treats log authorization
as mandatory so this cannot regress into a warning-only success.

## 2026-08-11 — fix diagnostics are pure, tested and harder to overfit

Bug-report sanitisation has moved out of the filesystem-writing CLI into a pure
module with adversarial tests. The previous implementation removed only one
double-quoted `data-testid` form, one locator form, Windows paths and
`localhost`; raw browser-console errors bypassed it completely. A fix session
could therefore receive single-quoted selectors, role/text locators, loopback
aliases, POSIX harness paths, stack frames, test timings, or credentials copied
from console output.

Behavioural diagnostics and console errors now share one sanitizer. It removes
those mechanics and redacts common credential forms while retaining useful user
observations from Playwright call logs. Contract-hook failures remain in their
own explicit section because the test id is the requirement in that case. The
focused suite passes 41/41, and the offline build→grade→report→fix→regrade loop
still passes with behavioural mechanics absent from the report.

The first clean-checkout workflow was added in the same pass. Linux installs
from the Stack Bench lockfile, installs Chromium and its system dependencies,
validates scenarios, runs focused safety/correctness tests and repeats the
offline orchestration loop three times. Windows runs the focused suite and the
same build→grade→report→fix→regrade orchestration once. The workflow YAML passes
an independent parser check.

This does not close the CI roadmap item. The repository has mutation manifests
but no checked-in known-good reference apps, and the real mutation runner needs
a live reference app. Running only anchor/schema validation would not prove the
grader catches a defect, so mutation CI remains explicitly blocked on those
fixtures rather than being represented by a weaker substitute.

## 2026-08-10 — runtime identity and session provenance are now durable

The build image is no longer trusted by tag. Its Node base is pinned by manifest
digest, the PostgreSQL and MongoDB service images are pinned the same way, and
each coding session resolves its configured build reference once and executes
the resulting immutable content ID. The run records that ID alongside the
container's Node and Claude Code versions, each database container's actual
image ID, the Linux SpacetimeDB CLI's binary hash and commit, and a source-tree
hash for the TypeScript SDK.

Each build, upgrade and fix session now records SHA-256 identities for the exact
prompt, selected skill text, contract appendix, track manifest, complete
scenario files and point-bearing rubric. Fix sessions additionally bind the bug
report they received. Individual fix sessions are retained instead of being
collapsed into one cost, and level/run totals now include every session's cost,
tokens, token classes, turns, prompt bytes, reasoning volume and model time.

The implementation is covered by deterministic hash and aggregation unit tests.
The focused suite passes 37/37 and the model-free build→grade→fix→regrade loop
proves the new session schema and totals end to end. The image was then rebuilt
from the digest-pinned Dockerfile and passed the real publish, SQL, watcher,
lease and cleanup smoke while executing by content ID. Restart-boundary fault
injection also passed against that rebuilt image while foreign resources
survived.

## 2026-08-10 — fatal cleanup is now exercised against real Docker resources

`npm run test:faults` now starts a dedicated repository-built SpacetimeDB host,
prepares the real build container, injects a coding-agent failure after both are
leased, and verifies that the host, container, resource locks and private lease
are removed. A separately owned HTTP listener and Docker container remain alive
through cleanup. The same gate first creates a same-name foreign container and
proves the launcher refuses it without changing it.

Building that gate exposed two ownership defects before the test passed. The
container launcher deleted stopped same-name containers by name and could adopt
an already-running same-name container into a fresh lease; it now reuses only an
immutable id already authenticated by that lease. The runner also performed a
host command-line process sweep even for container runs. Removing the host
runtime allowed that destructive Windows-specific sweep to be deleted.

The diagnostic host model-session path has been removed. The offline stub loop
still tests orchestration without a model, while all real coding sessions,
smokes and fatal-path tests use the single Docker lifecycle.

The restart-boundary test then found two Windows/WSL defects in the actual
restart script: the lease path crossed WSL twice and arrived at `node.exe` as a
mangled `C;...` path, and WSL looked for `taskkill` while Windows exposes
`taskkill.exe`, causing it to try a Linux `kill` against a Windows PID. The
scripts now translate the lease exactly once, use the Windows executable when
present, and verify the ping endpoint is truly down. Fault injection now stops
the leased host through that real script, exits before replacement, and proves
safe cleanup from the `restarting` state. This signs off Phase 0.

## 2026-08-10 — Docker was failing before `spacetime dev` got a fair test

Host execution is no longer a valid measurement path: real runs are
container-or-fail. A model-free Docker smoke now starts a dedicated host,
prepares the real build image, runs `spacetime dev`, publishes a TypeScript
module, verifies it through SQL, checks that the watcher remains alive, and
cleans up only its leased resources.

That smoke found that the repository SDK had been mounted read-only as a
`file:` dependency without its transitive runtime dependencies. The first
reported failure was an unresolved `headers-polyfill` import. The build
container now stages a fresh SDK copy and installs its production dependencies
before the coding session starts; non-Spacetime backends no longer see the SDK
or CLI at all.

With staging fixed, build, publish, watch and SQL verification pass in Docker.
Integrated log streaming did not at the time: the CLI directly logged in while
publishing, then obtains a different identity for the log stream, which is
rejected as unauthorized and retried every ten seconds. This is a product
finding, not evidence that the container hung, and the smoke reports it
separately as `logStreamingAuthorized: false`.

Resolved 2026-08-11 by retaining the publisher's authenticated `Config` through
the log-stream startup; see the newer entry above.

The same pass replaced boolean ownership with an authenticated backend lease.
It records the run id, destructive database/module target, dedicated host URI
and data directory, listener PID, database-container identity, and build-
container identity. Reset, restart and teardown now validate those identities;
a name or port match alone is insufficient authority to delete or kill. Lease
acquisition also takes atomic locks for the run slot and dedicated listener, so
two concurrent processes cannot claim the same database/module or port window;
dead-owner locks are reclaimed without stealing a live owner's lock.

Default result directories and transcript archives are now run-stamped rather
than reused by backend/index. `run.json` is written by atomic rename and carries
the producing run id; readers reject a stale id, and an explicit `--out` that
already contains a run is refused. Cross-backend behavioural review resolves
the newest stamped transcript for each requested comparison prefix.

Run and level records now carry structured outcomes instead of inferring
meaning from `0/0`, a missing bundle, or an error string. Harness failure,
ungraded, app failure, inconclusive, and pass are distinct; mixed app failures
and inconclusive criteria retain both lists. Run aggregation prioritizes a
harness failure without converting it into a backend score.

## 2026-08-10 — The harness needs a production gate before it needs more levels

The first production-hardening pass found three ways a run could look valid
when it was not: generated application config could choose the target of a
destructive Spacetime reset, stale phase artifacts could be consumed as current
output, and a fix round could replace conclusive evidence with INCONCLUSIVE
without being rolled back. Setup-wide grading failures could also leave their
points in the denominator.

The immediate safeguards are now in place: destructive resets require explicit
harness ownership and a loopback URI; module targets are derived by the harness;
pre-existing Spacetime hosts are refused; phase outputs are removed before use;
lost evidence invalidates a fix round; and setup-level inconclusive criteria are
removed from the available score. Scoring comparison is isolated in a pure
module with regression tests.

This does **not** make the benchmark production-ready. Only L1-L2 have been
exercised, and even those results remain diagnostic until rerun through the
frozen harness. L3-L5 are now explicitly experimental. Publishable comparisons
are gated on per-run backend leases, mandatory build isolation, atomic
run-stamped artifacts, structured harness-failure outcomes, clean-checkout CI,
and resolving the current criterion-traceability warnings. The roadmap now
promotes levels in order: freeze L1-L2, then concurrency/atomicity (L3), deferred
work and durability (L4), and production load/topology (L5).

The hardening slice was verified with 12 focused tests, including destructive
reset refusal and score-regression cases; a real two-phase offline fix loop
(one-fix-round and zero-fix-round) completed successfully; all 36 benchmark JSON
files parsed; and scenario validation completed while still reporting 69
`statedBy` warnings. Those warnings are remaining work, not a green production
signal. A root lockfile now pins Playwright and documents browser bootstrap, so
the harness at least has a reproducible local dependency baseline.

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
