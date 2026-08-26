# @spacetimedb/presence

Presence primitives for SpacetimeDB modules.

## Install

```bash
npm install @spacetimedb/presence spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

This package provides:

- reusable presence table row/builders,
- helpers for heartbeats and status/activity updates,
- bounded sweep helpers for expired presence rows.

## Usage

### Integrate into an application

Presence is a host-configured helper: the host chooses whether rows are public,
defines the scheduled sweep, and derives subjects from its authentication
model. The skeleton below owns those decisions explicitly.

```ts
import { Range, schema, t, table } from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import {
  createPresenceEntryTable,
  createPresenceConfigTable,
  installPresenceConfig,
  presenceSweepTickRow,
  runPresenceSweep,
  upsertPresence,
} from '@spacetimedb/presence';

const presenceEntry = createPresenceEntryTable({ public: true });
const presenceConfig = createPresenceConfigTable({ public: false });
const presenceSweepTick = table(
  { name: 'presence_sweep_tick' },
  presenceSweepTickRow
);

const spacetimedb = schema({
  presenceEntry,
  presenceConfig,
  presenceSweepTick,
});

export const init = spacetimedb.init(ctx => {
  installPresenceConfig(ctx);
  ctx.db.presenceSweepTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(30n * 1_000_000n),
  });
});

export const heartbeat = spacetimedb.procedure(
  { scope: t.string(), status: t.option(t.string()) },
  t.unit(),
  (ctx, args) => {
    ctx.withTx(tx => {
      upsertPresence(tx, {
        scope: args.scope,
        subject: ctx.sender.toHexString(),
        status: args.status ?? 'online',
      });
    });
    return {};
  }
);

export const presence_sweep = spacetimedb.reducer(
  { onSchedule: presenceSweepTick },
  { arg: presenceSweepTick.rowType },
  ctx => {
    runPresenceSweep(
      ctx,
      ctx.db.presenceEntry.expiresAt.filter(
        new Range(undefined, { tag: 'included', value: ctx.timestamp })
      )
    );
  }
);

export default spacetimedb;
```

The public operation derives its subject from `ctx.sender`. Applications with
account authentication can use a verified session's stable user ID instead.
See the
[Presence Chat host module](./example/spacetimedb/)
for authenticated subjects, typing scopes, and bounded cleanup.

After generating bindings, send heartbeats through the host procedure and
subscribe to the host's public or caller-scoped presence table:

```ts
import { tables } from './module_bindings';

await conn.procedures.heartbeat({
  scope: 'room:42',
  status: 'online',
});

conn
  .subscriptionBuilder()
  .subscribe([tables.myPresenceEntries.where(row => row.scope.eq('room:42'))]);
```

## API

- `createPresenceEntryTable` and `createPresenceConfigTable` create the host
  tables.
- `installPresenceConfig` installs default expiration policy.
- `upsertPresence` records a heartbeat or status change.
- `touchPresence` extends an existing lease while preserving its metadata.
- `removePresence` removes one scope and subject pair.
- `buildPresenceKey` creates the collision-safe compound key used by the
  default tables.
- `sweepPresence` removes expired rows from a supplied iterator.
- `runPresenceSweep` removes a bounded batch from an expiration-index iterator
  supplied by the host.
- `resolvePresenceSweepBatch` validates configured cleanup batch sizes.
- `presenceEntryRow`, `presenceConfigRow`, `presenceSweepTickRow`, and
  `presenceTables` support lower-level table composition.
- `DEFAULT_PRESENCE_TTL_SECONDS`, `DEFAULT_PRESENCE_SWEEP_BATCH`,
  `MAX_PRESENCE_SWEEP_BATCH`, and `DEFAULT_PRESENCE_STATUS` expose the package
  limits and defaults.

Package entrypoints:

- `@spacetimedb/presence` exports the full standalone helper surface.
- `@spacetimedb/presence/presence` exports presence operations.
- `@spacetimedb/presence/tables` exports table builders.
- `@spacetimedb/presence/submodule` exports the ready-made mounted
  namespace.

The ready-made namespace publishes `presence_entry` rows. The `activity` and
`payloadJson` fields are visible to subscribed clients. Do not store secrets or
private application data in these fields. Its manual sweep and configuration
operations require a presence administrator. Sweep batches are limited to
10,000 rows per call.

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
