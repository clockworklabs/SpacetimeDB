# @spacetimedb/presence

Show who is online and what they are doing in your SpacetimeDB application.
Use presence for an online-user list, room membership indicators, or typing
indicators in chat.

Clients send regular updates, called heartbeats, to stay listed as active.
Their presence expires when those updates stop. Your application can keep
separate lists for rooms or groups and control who sees them.

## Install

```bash
npm install @spacetimedb/presence spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

## Integrate into an application

Add Presence to your module and initialize it. This example lets a room member
send a heartbeat and see who is active in their rooms. Presence records are
private, so your operations and views must check room membership before writing
or returning them.

```ts
import { schema, SenderError, t } from 'spacetimedb/server';
import * as presence from '@spacetimedb/presence/submodule';

const spacetimedb = schema({ presence });

export const init = spacetimedb.init(ctx => {
  presence.install(ctx.as.presence);
});

export const roomHeartbeat = spacetimedb.reducer(
  { roomId: t.u64() },
  (ctx, { roomId }) => {
    if (!isRoomMember(ctx, roomId)) throw new SenderError('room.not_member');
    presence.upsertPresence(ctx.as.presence, {
      scope: `room:${roomId}`,
      subject: ctx.sender.toHexString(),
      status: 'online',
    });
  }
);

export const myRoomPresence = spacetimedb.view(
  { name: 'my_room_presence', public: true },
  t.array(presence.presenceEntry.rowType),
  ctx =>
    roomIdsFor(ctx).flatMap(roomId => [
      ...ctx.db.presence.presenceEntry.scope.filter(`room:${roomId}`),
    ])
);

export default spacetimedb;
```

`isRoomMember` and `roomIdsFor` stand for the host's own authorization. The
subject can be any stable ID; applications with account authentication can
use a verified session's user ID instead of the identity. See the
[Presence Chat host module](./example/spacetimedb/) for authenticated
subjects, typing scopes, and bounded cleanup.

After generating bindings, send heartbeats through the host operation and
subscribe to the host's view:

```ts
import { tables } from './module_bindings';

await conn.reducers.roomHeartbeat({ roomId: 42n });

conn.subscriptionBuilder().subscribe([tables.myRoomPresence]);
```

## API

Submodule operations:

- `heartbeat({ status, activity, payloadJson, ttlSeconds })` records the
  caller's presence in the global scope, keyed by its identity.
- `clear_presence()` removes the caller's global presence.
- `presence_online`: public view of up to 1,000 global presence entries,
  including their `activity` and `payloadJson`. Do not store secrets or
  private application data in those fields.
- `update_config({ defaultTtlSeconds, sweepBatch })`, `run_sweep()`, and the
  `presence_entries_admin` view require a presence administrator.
- `add_presence_admin({ identity })` and `remove_presence_admin({ identity })`
  manage administrators. The last administrator cannot be removed.

Helpers, called with `ctx.as.presence` or `tx.as.presence`:

- `upsertPresence` records a heartbeat or status change. `ttlSeconds` defaults
  to the configured `defaultTtlSeconds` (30 seconds unless updated).
- `touchPresence` extends an existing lease while preserving its metadata.
- `removePresence` removes one scope and subject pair.

Invalid input throws a `SenderError` with a code from the exported `errors`
object. Leases last at most one hour, payloads at most 4,096 characters, and
sweep batches at most 10,000 rows.

Package entrypoints:

- `@spacetimedb/presence` exports `errors` and the helpers.
- `@spacetimedb/presence/presence` exports the helpers.
- `@spacetimedb/presence/tables` exports the presence entry and config row
  definitions.
- `@spacetimedb/presence/submodule` exports the submodule namespace,
  `install`, operations, and helpers.
- [`spacetimedb/`](./spacetimedb/) publishes the submodule as a standalone
  database.

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

[Apache-2.0](./LICENSE.txt).
