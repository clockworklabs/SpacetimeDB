# Stack Bench findings

Issues surfaced by the benchmark harness, with reproduction steps.

---

## 1. Client skill doc: token is read once, so a first-session user loses identity on reconnect

**Classification:** ~~documentation / guidance gap. **Not** an SDK or host defect~~
— **REOPENED 2026-08-10. Half of this is an SDK defect.** The original
classification is correct about the *host*: minting a fresh identity for a
connection that presents no credentials is what "anonymous" means, and the
builder's contract says so explicitly — `withToken` is documented as *"optional.
You can store the token returned by the `onConnect` callback to use in future
connections."* Storing it is the application's job, by design.

**But that contract covers the application's next connection, not the SDK's
own.** `ConnectionManager` reconnects by rebuilding from the stored builder:

```
connection_manager.ts:149   this.#buildManagedConnection(managed, managed.builder)
connection_manager.ts:302   const connection = builder.build();
```

and `grep withToken src/sdk/connection_manager.ts` returns **nothing**. The
manager records the issued token in its own state (`:309 token: connection.token`)
and the connection adopts it (`db_connection_impl.ts:909`) — then throws it away,
because the rebuild re-presents `builder.#token`, which for a first-session
client is `undefined`.

So on an automatic reconnect the SDK **discards the identity it was just issued**
and silently acquires a new one. The application never initiated that reconnect
and cannot intervene before it happens; no callback fires in between, and no
error is raised. Saving the token diligently does not help, because the stale
builder is what gets rebuilt.

**FIXED 2026-08-10 in `1f77a6985`.** The three automatic reconnect paths now
re-present the token through `#reconnectManagedConnection`. `retain()` and
`rebuild()` still take the caller's builder verbatim, because supplying a
*different* token is exactly what `rebuild()` is for — swapping an anonymous
session for a signed-in one, or logging out. Ships with
`tests/connection_manager_token_reuse.test.ts`, which was checked to **fail when
the change is reverted** (`expected undefined to be 'token-from-host'`) rather
than passing vacuously. Suite green at 290; typecheck unchanged at its 17
pre-existing errors.

Two things deliberately left alone, and worth a decision by whoever owns the SDK:

- **A stale token now fails loudly.** If the module was republished with
  `--delete-data` or the key rotated, re-presenting the old token errors through
  `onConnectError` instead of quietly becoming a new user. Better failure, but it
  is a behaviour change.
- **An identity change is still silent.** Even with the fix a reconnect can
  legitimately yield a different identity, and nothing tells the app. A warning
  or callback when the identity changes across a reconnect would turn silent
  data-loss into something an application can handle, and is worth doing
  independently.

**Scope — narrower than first written.** `ConnectionManager` is used by the
**React / Solid / Svelte providers**, not by a plain `DbConnection.builder()`
app. That is not a reprieve: it is the recommended path, it is what the client
skill doc prescribes, and it is what every generated app uses.

**Not new.** `ConnectionManager` landed 2026-02-19 (#4028), with reconnect fixes
in June (#5185, #5375). Roughly six months old and shipped.

**The prescribed pattern walks straight into it.** From the generated app's
`main.tsx` — and the client skill doc says the same thing:

```js
const connectionBuilder = useMemo(
  () => DbConnection.builder()
    .withUri(SPACETIMEDB_URI)
    .withDatabaseName(MODULE_NAME)
    .withToken(localStorage.getItem('auth_token') || undefined),
  []                       // built once, at first render
);
```

On a first-ever visit `localStorage` is empty, so the builder is frozen with
`#token = undefined` **forever**. The app then saves its token correctly, and it
makes no difference: `ConnectionManager` rebuilds from that same frozen builder.

**Which criteria this actually explains — corrected.** Of the six identity points
SpacetimeDB lost on first try, this mechanism accounts for **105b** ("across the
connection dropping and re-establishing"), which is precisely the automatic
reconnect. The reload criteria (1e, 4b, 105a) should *not* be affected: a page
reload re-runs `main.tsx`, so `useMemo` re-reads `localStorage` and picks the
token up. Those are more likely the app never saving the token at all — a
separate failure the skill doc already warns about at line 59. **The first-build
source is not preserved, so which of the two it was cannot be confirmed.**

The skill doc has been revised three times around this behaviour (`15e666e38`,
`edd8e8a27`, `fe9ba42d8`). When documentation has been "fixed" three times and
generated apps still get it wrong, the API shape deserves the scrutiny.

**Not yet proven for the benchmark specifically.** The first-build source and
first-build grading detail are not preserved — only the missed criterion IDs — so
the link between this mechanism and those six points is inference from what the
criteria assert, not a traced failure. **That is itself a harness gap:** we do
not keep the artifact needed to diagnose first-try failures, which is the thing
that most determines cost-to-correct.

**Severity:** medium — affects every app generated from the current client skill,
but only users who obtained their identity in the current page session.

**Symptom.** After the SpacetimeDB host restarts, a browser client that first
connected anonymously is issued a **new Identity** on reconnect. Rows owned by
the previous identity — its user record, its scheduled messages, anything
filtered by `ctx.sender` — become invisible, and an app that gates on "do I have
a user row" shows its registration screen again. No error is raised.

**Evidence.** Probing the same app with and without a host restart; the JWT
subject is the identity:

```
plain reload, no restart
  BEFORE sub: 613e49b0-389d-4dc0-852a-257dcf321625
  AFTER  sub: 613e49b0-389d-4dc0-852a-257dcf321625   identical, stays logged in

reload after `spacetimedb-standalone` restart
  BEFORE sub: 828b7285-7681-4278-b715-b582fa8c854f
  AFTER  sub: 6c0761cf-5ea4-4dcd-ba42-ef5f9c73b2eb   DIFFERENT, shows registration
```

The new token's `iat` falls inside the restart window, before the page reload, so
the new identity is minted on the SDK's automatic reconnect. The signing key
(`config/id_ecdsa`) is untouched, so it is not key rotation.

**Mechanism.** The canonical pattern in `skills/typescript-client/SKILL.md` reads
the token exactly once, when the builder is constructed:

```ts
// SKILL.md:33 — evaluated once, at page load
.withToken(localStorage.getItem('auth_token') || undefined)

// SKILL.md:57 — stores the token, but never feeds it back to the builder
useEffect(() => { if (token) localStorage.setItem('auth_token', token); }, [token]);
```

For a first-time visitor that first read yields `undefined`. The server issues an
identity and token, the app stores it, but the *builder* still carries
`undefined`. `ConnectionManager` reconnects by rebuilding from that same builder
(`connection_manager.ts:352`), so the reconnect presents no token and the server
correctly mints a fresh anonymous identity. A returning visitor is unaffected,
because by then the stored token is present at load — which is why a plain reload
works and hides the problem.

**Why this is not a bug.**

- A connection that presents no token is anonymous; issuing it a new identity is
  the only correct behaviour.
- The SDK deliberately treats the builder as the source of truth for auth.
  `connection_manager.ts:392` documents this and offers `rebuild()` as the
  supported escape hatch for "reconnect with a fresh token" flows, e.g. swapping
  an anonymous session for a signed-in one. Silently reusing the last-seen token
  would break exactly that flow, and would make logout unexpressible.

**Deliberately NOT fixed in the skill.** A workaround was written, verified, and
then reverted. Patching benchmark inputs in response to a failing test the same
harness defines is teaching to the test, and it would have improved a score
without improving the product. The gap belongs in the React integration — see
below — and the benchmark should keep reporting the failure until that lands.

**The workaround that was verified** (kept here as evidence the diagnosis is
correct, not as guidance to ship):

```ts
useEffect(() => {
  if (!token) return;
  localStorage.setItem('auth_token', token);
  // Ensure a later reconnect authenticates as the same identity.
  rebuild(DbConnection.builder()
    .withUri(SPACETIMEDB_URI)
    .withDatabaseName(MODULE_NAME)
    .withToken(token));
}, [token]);
```

**The real fix, in the SDK.** `ConnectionManager.rebuild()` is documented as the
supported "reconnect with a fresh token" path, but `ConnectionManager` is not
exported from the public index and the React layer does not surface it, so an app
using `SpacetimeDBProvider` cannot reach it. Exposing token updates through the
React integration — a `rebuild`/`setToken` on the context, or having the provider
honour a changed builder — would let apps do this properly and make the
workaround unnecessary.

**Why it matters.** Every deploy restarts the host. Any user who signed up during
the current session and has not reloaded since silently becomes a different user
and loses access to their own data. Because the guidance is ours, every generated
SpacetimeDB app inherits it — this very likely explains the
`identity-lost-on-refresh` defect recorded against SpacetimeDB apps in the
historical benchmark grading taxonomy, which was previously attributed to the
generated apps themselves.

**Benchmark impact.** The SpacetimeDB app scored 1/2 rather than 2/2 on
`scenarios/level-02-durability.json` for this reason. The scheduled message did
deliver — the schedule survived the restart — but the author's pending list read
empty because the author was, by then, a different identity. That is a legitimate
app-level failure, and it originates in our own documentation.

**Reproduce.**

```bash
# any generated SpacetimeDB chat app, dev server on :6173
node tools/stack-bench/grader/probe-identity.mjs http://localhost:6173 \
  "bash tools/stack-bench/restart-backend.sh spacetime <app-dir>"
# control (expect identical subjects):
node tools/stack-bench/grader/probe-identity.mjs http://localhost:6173
```

**Caveat.** Reproduced on one machine (Windows, host launched with
`--data-dir ...\SpacetimeDB\data --jwt-key-dir ...\SpacetimeDB\config`, restarted
via `spacetime start`). Worth confirming on Linux.

---

## Two PostgreSQL shops both score 50/50; only one survives a deploy

**Classification:** benchmark finding — what the feature score cannot see.

**Severity:** high for the product argument. The failure is invisible to every
functional test and shows up only under the withheld systems criteria.

Two one-shot PostgreSQL builds of the same ecommerce L1 spec, graded by the same
harness, both scored **50/50 on features and invariants**. Both implement live
updates with `LISTEN`/`NOTIFY`. They differ in where the `NOTIFY` is attached,
and that difference decides whether the app is correct.

| build | where NOTIFY lives | out-of-band write | deploy window (901c) |
|---|---|---|---|
| `postgres-ecom-run0` (earlier) | `CREATE TRIGGER` on the table | fires for any write | not measured — build is not runnable |
| `postgres-ecom-run0` (2026-08-09) | `pg_notify(...)` called in route handlers | fires only for the app's own writes | **FAILS** |

**Evidence.** 901c writes a stock correction directly to the database while the
app server is down, then brings the server back and reads an already-open
storefront. After the restart:

    database:  SELECT SUM(quantity) ... WHERE name = 'Desk Lamp'  ->  15
    storefront: [data-testid="item-stock"] inside "Desk Lamp"      ->  50

The write landed (15 = 5 East from 901a + 10 West from 901c). The client never
learned. 901a and 901b passed on the same build, so the app does propagate a
direct database write *while it is running* — the loss is specific to the window
where nothing is listening, and nothing reconciles on reconnect.

**Why this matters more than a missed trigger.** An event stream with no
reconciliation loses whatever happens while the listener is down. Every deploy,
every restart, every dropped connection is a window in which changes vanish from
every connected client and stay vanished until something forces a refetch. The
app cannot tell that it is stale, and neither can a functional test suite: this
build scored full marks.

**What it costs to get right on PostgreSQL.** Attaching NOTIFY to triggers
rather than to code paths, plus a reconcile-on-reconnect that refetches current
state instead of resuming a stream. The earlier build got the first half by
writing four trigger functions and four triggers; neither build got the second.

**Measured, not assumed.** SpacetimeDB was run against the same spec the same
day and passed 901c: its subscriptions are derived from committed state, so a
resubscribe after the restart re-read the corrected stock and the storefront
showed 15. 901c is therefore promoted to score — it has now failed one real
build and passed another, which discriminates more convincingly than a mutant.
901a, 901b and 902a pass on both stacks and stay at zero: a criterion nothing
has ever failed has not shown it can tell anything apart.

**A gap in the other direction.** The contention suite could not score
SpacetimeDB at all — both point-carrying criteria returned INCONCLUSIVE. 203b
replays captured HTTP writes, and this backend writes over WebSocket, so
nothing was contended; 201a lost one of six concurrent clicks before it
dispatched. SpacetimeDB scored 0/0 there rather than passing or failing. Until
that is fixed the contention axis measures PostgreSQL and MongoDB only, and no
cross-stack contention claim can be made from it.

**Confirmed independently, and it is not just slowness.** Re-run by hand against
the same build outside a graded pass, 901c failed again — and failed with the
assertion window raised from 20s to 90s. The page open across the restart never
reaches the right number.

**And the app can reconcile; it just never does it on that path.** 901d, added
to test the opposite window, PASSES on this build: with the server up and the
CLIENT disconnected, the correction appears as soon as the browser comes back.
So a client-initiated reconnect triggers a refetch and converges. A server
restart does not. 901d therefore discriminates nothing yet and stays at zero
points — but it is what proves 901c is a missing reconciliation path rather
than a missing capability.

Postgres systems result on this build: 901a pass, 901b pass, 901c FAIL,
901d pass, 902a pass.

---
## 2026-08-09 — grading audit: what each L1 invariant actually proves

Re-graded the preserved PostgreSQL build with the instrumented grader, which now
records per criterion whether an authorization check was `verified` (a hostile
request was issued and refused), `structural` (the write carries no
client-supplied identity, so there is nothing to forge), or `unverified` (the
replay could not be attempted; the pass rests on DOM assertions).

Of 13 L1 invariant criteria on PostgreSQL:

| depth | criteria | what a pass means |
|---|---|---|
| structural | 102a | the write path carries no client identity — the server must derive it |
| unverified | 101a, 103a, 104a, 109a | the hostile request could not be issued; DOM assertions only |
| DOM assertion | the remaining 8 | observed state (books balance, isolation, persistence) |

**Zero criteria are `verified` on this build.** Not one authorization invariant
actually issued a request the server then refused. That is not necessarily the
app being unsafe — for 104a (price integrity) and 102a it is the opposite: the
app is safe *because* it puts no price and no caller identity in the request, so
there is nothing to tamper with, and the replay correctly finds nothing to
retarget. Safety and "unverified" are the same fact here.

But it does mean the L1 invariant score is weaker evidence than 19/19 suggests.
It confirms the app behaves correctly through its own interface; it does not, on
this build, confirm the server refuses a request the interface would never send.

**What this changes:**

1. 104a and 109a now also assert `expectReplayRejected`, so when a replay CAN be
   issued the server's refusal is required, not just the resulting number. On
   this build the replay is still inconclusive (nothing to retarget), so the
   assertion is a no-op here and becomes real on an app that does carry the
   field.
2. The verified/structural/unverified label is now in every criterion record.
   A criterion record still does not copy the ctx-level reason string, so the
   label is trustworthy but its one-line explanation is not yet persisted —
   fix before leaning on the detail text.

**The honest headline:** L1 = 51/51 on all three stacks, and it means "builds a
correct app against a stated spec." It does NOT yet mean "the servers refuse
hostile requests," because on the reference build the hostile requests could not
even be constructed. Whether that is strength (nothing to attack) or a gap in
the test (we never confirmed refusal) differs per criterion and is now visible
per criterion instead of hidden inside a number.

---
## 2026-08-09 — L2 lint under-verifies (not a blocker, recorded before first run)

The L2 contract adds two lint stages, `fulfilment` (5 hooks) and `operations`
(11), which the ecommerce walk never visits — it covers the L1 golden path only.
Because lint.mjs only fails on hooks it actually marked FAIL or BLOCKED, and an
unvisited hook is never added to results, lint PASSES at L2 while silently
checking 8 of 24 hooks. It is a false pass, not an abort.

It does not corrupt scores: the grader's L2 scenarios click those hooks directly
(ship-submit, transfer-submit, queue-item, ...), so a missing hook fails its
criterion where it should. What is lost is fail-fast — a missing L2 control is
found during grading rather than in one cheap lint pass up front.

Fixing it means an L2 golden path in the walk: sign in as staff, place an order
so the queue is non-empty, ship it, do a transfer, change a price. That is real
and slightly fragile work; deferring until after the first L2 results rather
than risk a flaky walk mid-sweep. Recorded so the "CONTRACT LINT PASS" line on
an L2 run is read for what it is.

---
## 2026-08-10 — a fix round read the answer key, through Bash

The SpacetimeDB L2 run is void. Its fix-round session read the scenario file
that defines the criteria it was failing, read grade.mjs, and then ran the
grader itself — 23 accesses, every one through Bash:

    grep -n -A5 -B5 "staff-link\|1d\|1e" .../scenarios/02-features.json
    sed -n '1,260p'                        .../scenarios/02-features.json
    grep -n "replayAs\|expectReplayRejected\|CallReducer" .../grader/grade.mjs
    cd .../grader && node grade.mjs --url http://localhost:6473 --level 2 --spec ...

This is the documented hole, exercised for real. The sandbox denies the file
tools and allows Bash by design; leak-audit is the control, and it did its job —
the run was marked CONTAMINATED and no score was reported. But the control is
detective, not preventive: $12.66 was spent and the level produced nothing
usable.

**Retracted:** the unaided 48/54 from this run, which was quoted as the first
evidence that SpacetimeDB needs less fixing at L2 than PostgreSQL's 37/54. It
came from a run whose later session had read the specs. It cannot be used, and
the L1->L2 comparison currently has one valid arm (PostgreSQL) and none for
SpacetimeDB.

**Why this run and not the others.** Nothing in this level is special except
that the fix agent was failing criteria it could not diagnose from the app
alone — 1e (ship authorization) among them, which is the criterion added in
tonight's hardening. A harder, less familiar failure is exactly the condition
that makes reading the grader attractive. Expect this to recur on any level
where fixes are hard, not to be a one-off.

**Secondary bug, same run.** After the fix round scored lower, bench rolled the
source back and re-graded. The rollback restores source but not node_modules,
so the re-grade's database reset failed with "you may have forgotten to install
dependencies" and the level ended NOT GRADED. Two separate defects: the
contamination, and a rollback that cannot produce a bundle.

**Also unsound, found in the same log.** The regression check compares raw
scores across passes whose denominators differ: unaided 48/54 against fix-round
46/53. Those are 88.9% and 86.8% — the direction happened to be right, but
comparing numerators when max can move is not a valid test, and it is what
triggered the rollback.

---
## 2026-08-10 — L2 sweep: two of three runs lost to one harness defect

| stack | L2 features | L1 guarantees | outcome | cost |
|---|---|---|---|---|
| postgres | 37/54 -> 54/54 (2 rounds) | 20/20 held | VALID | $13.84 |
| spacetime | 48/54 unaided | 20/20 held | VOID — read the grader | $12.66 |
| mongodb | 54/54 unaided | **18/20 — 2 LOST** | NOT GRADED — rollback broke | $21.40 |

$34 of the $48 spent produced no usable score, and both failures trace to
defects fixed after the sweep had already started (Node loads bench.mjs once).

**MongoDB is the result worth keeping even though the run is unusable.** Its
FIRST pass scored 54/54 on everything L2 asked for while losing two L1
guarantees, one of them 109a — one customer's cart request replayed by another
leaves the owner's cart untouched. A benchmark scoring only the current level
would have called that a flawless upgrade. The inherited-guarantee suites are
the only reason it is visible. That is the clearest evidence so far that
guarantees erode as an app grows, and it appeared on MongoDB rather than on the
stack the thesis expected.

Its fix round then regressed the app catastrophically — 54/54 to 20/54, features
36 to 10 — trying to repair those two guarantees. Rollback fired correctly and
then failed, because restoreSource deleted node_modules along with the source.

**Not yet established.** Whether MongoDB's two lost guarantees are genuine app
regressions or oracle flakiness. 109a passes at `unverified` depth on some
stacks, meaning the hostile replay could not be issued and the criterion rests
on DOM assertions — a criterion in that state can fail for reasons that are not
the app's fault. This needs the re-grade before it is quoted as a finding.

**SpacetimeDB has no valid L2 arm**, so the comparison that motivates the whole
ladder is still missing its most important column.
