# @spacetimedb/rate-limit

Fixed-window rate limiter submodule for SpacetimeDB TypeScript modules.

## Install

```bash
npm install @spacetimedb/rate-limit spacetimedb
```

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This package gives you:

- a `./submodule` namespace with submodule-owned bucket/config/admin tables
- configured limiter clients that consume and read buckets in host transactions
- a scheduled, bounded sweep of expired buckets
- admin-gated operations for diagnostics and maintenance

## Usage

### Integrate into an application

Register the namespace, install its scheduled cleanup and admin state, then
consume from a limiter before performing the protected action:

```ts
import { schema, SenderError, t, table } from 'spacetimedb/server';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';

const post = table(
  { name: 'post', public: true },
  {
    id: t.u64().primaryKey().autoInc(),
    author: t.identity(),
    body: t.string(),
    createdAt: t.timestamp(),
  }
);

const spacetimedb = schema({
  rateLimit,
  post,
});

export const init = spacetimedb.init(ctx => {
  rateLimit.install(ctx.as.rateLimit);
});

export default spacetimedb;
```

Configure each policy once at module scope, then consume inside the same
transaction as the protected write. Derive the actor key from trusted request
or session state:

```ts
const postLimiter = rateLimit.client({
  scope: 'post.create',
  limit: 10,
  windowSeconds: 60,
});

export const createPost = spacetimedb.procedure(
  { body: t.string() },
  t.unit(),
  (ctx, args) => {
    ctx.withTx(tx => {
      const actor = ctx.sender.toHexString();
      const result = postLimiter.consume(tx.as.rateLimit, { key: actor });
      if (!result.allowed) throw new SenderError('post.rate_limited');
      const body = args.body.trim();
      if (!body) throw new SenderError('post.empty');
      tx.db.post.insert({
        id: 0n,
        author: ctx.sender,
        body,
        createdAt: ctx.timestamp,
      });
    });
    return {};
  }
);
```

The generated client calls the product-facing operation:

```ts
await conn.procedures.createPost({ body: 'Hello' });
```

A view can report the caller's remaining capacity with `peek`:

```ts
export const postLimitStatus = spacetimedb.view(
  { name: 'post_limit_status', public: true },
  t.array(
    t.object('PostLimitStatus', {
      limit: t.u32(),
      windowSeconds: t.u32(),
      used: t.u32(),
      remaining: t.u32(),
      resetAt: t.option(t.timestamp()),
    })
  ),
  ctx => [postLimiter.peek(ctx.db.rateLimit, ctx.sender.toHexString())]
);
```

Mounted as `rateLimit`, the submodule owns these tables:

- `rate_limit.rate_limit_bucket`
- `rate_limit.rate_limit_admin_identity`
- `rate_limit.rate_limit_config`
- `rate_limit.rate_limit_sweep_tick`

It also exposes the public admin view `rate_limit.admin_rate_limit_buckets`.
The view returns at most 1,000 rows and returns an empty set to non-admins.

## API

`client({ scope, limit, windowSeconds })` configures the policy for one scope.
Each scope can be configured once per module; a second `client` call with the
same scope fails module loading with `rate_limit.duplicate_scope`.
`resetRegisteredScopes()` clears the configured scopes, for example between
tests that configure the same scope again. When a
policy depends on application state, such as an upgrade tier, create one client
per tier with its own scope.
The returned limiter exposes `scope`, `limit`, and `windowSeconds`, plus:

- `consume(ctx, { key, cost? })` spends `cost` (default 1) from the actor's
  bucket in the caller's transaction and returns whether it was allowed, the
  remaining capacity, the reset time, and the retry delay. `cost` must be a
  positive integer no greater than `limit`; otherwise `consume` throws
  `errors.invalidCost`, since the request could never be allowed.
- `peek(db, key, now?)` reads the actor's bucket without spending from it.
  Views have no clock, so a view reports an expired window until the sweep
  removes it; compare `resetAt` with the current time, or pass `now` from a
  reducer or procedure to report an expired window as fresh.

`key` identifies the actor; the limiter combines it with the scope. Scopes are
at most 128 characters. A key must be 1 to 256 characters; otherwise `consume`
and `peek` throw `errors.invalidActorKey`.

`install(ctx)` makes the publishing identity the first administrator and
schedules a sweep of expired buckets every 30 seconds. Call it from the host's
`init` reducer. Repeated installation does not add administrators or timers.

Admin operations:

- `isAdmin(db, identity)` and `requireAdmin(ctx)` check the submodule's admin
  table for host operations that share its administrators.
- `addRateLimitAdmin` and `removeRateLimitAdmin` grant and revoke access. The
  last administrator cannot be removed.
- `updateConfig({ sweepBatch })` sets how many expired rows each scheduled sweep
  deletes: 500 by default, at most 10,000.
- `runSweep({ maxRows })` deletes expired buckets now, and
  `resetBuckets({ maxRows })` deletes buckets regardless of expiry. Both remove
  500 rows by default and accept at most 10,000 per call.

`errors` holds the stable codes a caller can receive as a `SenderError`:
`notAuthorized`, `cannotRemoveLastAdmin`, `invalidActorKey`, `invalidCost`,
`invalidSweepBatch`, and `invalidMaxRows`. An invalid policy passed to `client`
fails module loading instead.

Package entrypoints:

- `@spacetimedb/rate-limit/submodule` supplies the submodule namespace,
  `install`, the limiter client, and the admin operations.
- `@spacetimedb/rate-limit` exports `client`, `errors`, and their types.

See the
[Powerhouse host module](./example/spacetimedb/)
for per-action and per-tier policies, caller-visible status, and admin controls.

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

[Apache-2.0](./LICENSE.txt).
