# @spacetimedb/rate-limit

Fixed-window rate limiter submodule for SpacetimeDB TypeScript modules.

## Install

```bash
npm install @spacetimedb/rate-limit spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This package gives you:

- a mountable `./submodule` with submodule-owned bucket/config/admin tables
- standalone helper functions for direct host integration
- bounded sweep helpers for expired buckets
- admin-gated procedures for diagnostics and maintenance

## Usage

### Integrate into an application

Mount the namespace, install its scheduled cleanup and admin state, then call
`consume` from the host operation before performing the protected action:

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
  rateLimit.installRateLimit(ctx.as.rateLimit);
});

export default spacetimedb;
```

Host procedures should call the standalone policy helper inside the same
transaction as the protected write. Derive the actor key from trusted request
or session state:

```ts
export const create_post = spacetimedb.procedure(
  { body: t.string() },
  t.unit(),
  (ctx, args) => {
    ctx.withTx(tx => {
      const scope = 'post.create';
      const actor = ctx.sender.toHexString();
      const result = rateLimit.consumeRateLimit(tx.as.rateLimit, {
        key: rateLimit.buildRateLimitKey(scope, actor),
        scope,
        limit: 10,
        windowSeconds: 60,
      });
      if (!result.allowed) throw new SenderError('rate_limit.blocked');
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

The submodule owns these tables under the mounted namespace:

- `rateLimit.rate_limit_bucket`
- `rateLimit.rate_limit_admin_identity`
- `rateLimit.rate_limit_config`
- `rateLimit.rate_limit_sweep_tick`

It also exposes the public admin view `rateLimit.admin_rate_limit_buckets`.
The view returns at most 1,000 rows and returns an empty set to non-admins.

## API

The root package exports lower-level helpers for custom standalone
implementations:

- `consumeRateLimit`
- `buildRateLimitKey`
- `installRateLimitState`
- `runRateLimitSweep`
- `sweepRateLimits`
- `resolveRateLimitSweepBatch`

The mounted `consume`, `runSweep`, and `reset_buckets` operations are admin-only.
Application-facing operations should enforce a fixed policy in host code and use
`consumeRateLimit` as shown above. `reset_buckets({ maxRows })` removes
1,000 rows by default and accepts a maximum of 10,000 per call, so destructive
maintenance remains bounded.

Those helpers expect the same submodule table shape. Namespace-aware modules
use `@spacetimedb/rate-limit/submodule`.

Package entrypoints:

- `@spacetimedb/rate-limit/submodule` supplies the mounted namespace,
  maintenance operations, and host helpers.
- `@spacetimedb/rate-limit/limit` exports standalone policy functions.
- `@spacetimedb/rate-limit` re-exports the supported helper surface.

See the
[Powerhouse host module](./example/spacetimedb/)
for per-action policies, caller-visible status, and admin controls.

## Exported Defaults

- `DEFAULT_SWEEP_BATCH = 500`
- `DEFAULT_SWEEP_INTERVAL_SECONDS = 30n`

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
