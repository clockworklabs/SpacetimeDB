# Stack Bench findings

Product issues surfaced by the benchmark harness, with reproduction steps.

---

## 1. TypeScript SDK: auto-reconnect silently changes a client's identity

**Severity:** high — affects any browser client that first connects anonymously,
which is the default for a new user.

**Symptom.** After the SpacetimeDB host restarts, a connected client is issued a
**new Identity**. Rows owned by the previous identity (its user record, its
scheduled messages, anything filtered by `ctx.sender`) become invisible to it,
and an app that gates on "do I have a user row" shows its registration screen
again. No error is raised; the client appears to reconnect normally.

**Evidence.** Probing the same app, with and without a host restart. The JWT
subject is the identity:

```
plain reload, no restart
  BEFORE sub: 613e49b0-389d-4dc0-852a-257dcf321625
  AFTER  sub: 613e49b0-389d-4dc0-852a-257dcf321625   identical, app stays logged in

reload after `spacetimedb-standalone` restart
  BEFORE sub: 828b7285-7681-4278-b715-b582fa8c854f
  AFTER  sub: 6c0761cf-5ea4-4dcd-ba42-ef5f9c73b2eb   DIFFERENT, app shows registration
```

The new token's `iat` falls inside the restart window, before the page reload —
so the new identity is minted on the SDK's automatic reconnect, not on reload.
The signing key (`config/id_ecdsa`) is untouched by the restart, so this is not
key rotation.

**Mechanism.** `ConnectionManager` reconnects by rebuilding from the *original*
builder:

- `connection_manager.ts:352` — `this.#buildManagedConnection(managed, managed.builder)`
- The builder holds whatever `withToken()` was given at page load
  (`db_connection_builder.ts:81`). For a first-time visitor that is `undefined`.
- The token the server issues during the session is recorded on the connection
  and in managed state (`connection_manager.ts:243`, `db_connection_impl.ts:908`)
  but is **never written back to the builder**.

So the reconnect presents no token and the server mints a fresh anonymous
identity. A returning visitor is unaffected, because by then the app has stored a
token and passes it to `withToken()` at load — which is why a plain reload works
and masks the bug.

**Suggested fix.** Carry the acquired token into the rebuilt connection, e.g. in
`#scheduleReconnect` before rebuilding:

```ts
const token = managed.state?.token;
if (token) managed.builder.withToken(token);
this.#buildManagedConnection(managed, managed.builder);
```

**Why it matters beyond this benchmark.** Every deploy restarts the host. Any
user who signed up in the current session and has not reloaded since silently
becomes a different user, losing access to their own data. This likely explains
the `identity-lost-on-refresh` defect recorded against SpacetimeDB apps in the
historical benchmark grading taxonomy.

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
via `spacetime start`). Worth confirming on Linux and against a host restarted by
whatever mechanism production uses before treating the mechanism above as final.
