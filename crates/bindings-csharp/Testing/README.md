# Managed Module Testing

See [Unit Testing](https://spacetimedb.com/docs/how-to/unit-testing) for a guide.
Supported platforms are Linux x64 and ARM64 (glibc 2.35 or later) and Windows x64.

This .NET 10 project provides `ModuleTestContext.Create(typeof(Module).Assembly)`,
fixture-owned `Db`, and synchronous `WithReducerTx` callbacks backed by the native
test datastore. Use `TestAuth.Internal()` for the database identity, or
`TestAuth.FromJwtPayload(json, connectionId)` for a client. `auth.Sender` and
`auth.ConnectionId` expose the caller; both are null for `TestAuth.Internal()`.
Client credentials require a nonzero connection ID and string `iss`/`sub` claims of 1-128 UTF-8 bytes.
An optional `hex_identity` must match the derived identity. Other claims are
preserved for the existing `JwtClaims` API; signatures and expiry are not checked.
Credentials are reusable; parsed claims are per invocation.
Transactions commit on normal return and roll back on exceptions; their handles are invalid
after the callback finishes. Default clocks start at the Unix epoch, and RNGs
start at root seed zero. `Clock.Set`/`Advance` and `Rng.SetSeed` control
subsequent contexts. Managed module logging writes to standard error.

`SetEnv(key, value)` and `UnsetEnv(key)` configure fixture-local environment values
declared by the root module. Reads see current values, and transaction rollback does
not undo environment changes. Environment handles expire with their owning context.

`CreateViewContext(auth)` and `CreateAnonymousViewContext()` create fixture-owned read-only
contexts for calling module views directly. `RunQuery<T>(IQuery<T>)` executes their
typed SELECT queries, including semijoins, against committed tables and returns
materialized rows. Row types must implement `IStructuralReadWrite` and have a
parameterless constructor, as generated module rows do. Queries execute with the
database identity; caller filtering comes from the view's `Sender` and query.
The native bridge rejects queries while a mutable transaction is active.
Contexts expire on fixture disposal; materialized results remain ordinary values.

`CreateProcedureContext(auth)` creates a context for calling procedures directly.
`WithTx` commits a returned value and rolls back/rethrows exceptions; `TryWithTx`
also rolls back a returned `Result.Err` and reports failures through `TxOutcome`.
There is no transaction around the whole procedure, so earlier commits survive
later failures. Each transaction gets fresh handles and a timestamp from the
fixture clock at entry. Auth and RNG are shared with the procedure, and environment
reads remain live. Escaped transaction handles never become valid again.
Use `tx.Db` inside the callback, not `procedure.Db`; nested transactions and
async/awaitable callbacks are rejected.

Call `AfterTxCommit(fixture => { ... })` on `CreateProcedureContextBuilder(auth)` to run
a synchronous callback after each successful procedure transaction has committed and closed.
Hooks can access committed data and advance
the fixture clock for the next transaction. They do not run after rollback.
Hook exceptions propagate directly from both `WithTx` and `TryWithTx`, without
undoing the commit or being converted into `TxOutcome` failures. Built contexts
capture their hook; later changes to the builder do not affect them.
Calling `AfterTxCommit` again replaces the configured callback. There is no sleep hook.

Use `CreateProcedureContextBuilder(auth).Http((fixture, request) => ...).Build()` to
install a per-context HTTP responder returning `Result<HttpResponse, HttpError>`.
It receives the existing SpacetimeDB request types, including the body. Non-2xx
responses remain `Ok`; transport failures are `Err`. Responder exceptions and
assertion failures propagate unchanged. Without a responder, HTTP returns
`no test HTTP responder configured` and never calls the network or host imports.
HTTP is rejected while any transaction on that fixture is active. Empty URIs
and negative timeouts are rejected, and timeouts above 180 seconds are clamped
without changing the caller's request. Built contexts retain their responder
even if the builder is later reconfigured; fixture disposal invalidates HTTP.

Use one execution flow per fixture. Async callbacks and multicast delegates are
rejected; returned `Task`/`ValueTask` values also cause rollback. Independent
fixtures may run in parallel. Table deletion and clearing use the owning
transaction, and the engine removes event rows on commit. Unique-index lookup,
update, and deletion use fixture-owned index resolution, including mounted modules.
Range filtering and deletion support the existing inclusive bounds and composite
prefixes, with owner-lifetime checks on iterators. Unsupported host services fail
explicitly; lifecycle and scheduled reducers must be invoked directly.
