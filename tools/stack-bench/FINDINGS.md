# Stack Bench findings

Issues surfaced by the benchmark harness, with reproduction steps.

---

## 1. Client skill doc: token is read once, so a first-session user loses identity on reconnect

**Classification:** documentation / guidance gap. **Not** an SDK or host defect —
see "Why this is not a bug" below.

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
