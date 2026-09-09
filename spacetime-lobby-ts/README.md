# @spacetimedb/lobby

SpacetimeDB lobby and matchmaking submodule.

This package provides queue tickets, deterministic same-pool matchmaking,
ranked two-player results with Elo ratings, rooms, seats, lifecycle state,
admin observability, and submodule helpers for host modules. Host applications
define parties, backfill, and product-specific match rules.

## Install

```bash
npm install @spacetimedb/lobby spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

## Usage

### Integrate into an application

For a host application, register the namespace and keep the lifecycle hook in the
host module:

```ts
import { schema } from 'spacetimedb/server';
import * as lobby from '@spacetimedb/lobby/submodule';

const spacetimedb = schema({ lobby });

export const init = spacetimedb.init(ctx => {
  lobby.installLobby(ctx.as.lobby);
});

export default spacetimedb;
```

Host modules can call submodule helpers with an explicit subject after they have
validated auth or mapped the SpacetimeDB identity to an application user ID:

```ts
lobby.joinQueue(ctx.as.lobby, {
  pool: 'duel',
  subject: userId,
  matchSize: 2,
  attributesJson: JSON.stringify({ region: 'iad' }),
});
```

Public submodule reducers derive the subject from `ctx.sender.toHexString()`.
Host wrappers can map the caller to an application user ID.
See the
[Starclash host module](./example/spacetimedb/)
for profile mapping, matchmaking, match results, and caller-scoped views.

After generating bindings, a client joins through the host operation and reads
match state through subscriptions:

```ts
import { tables } from './module_bindings';

await conn.reducers.findDuel({});

conn
  .subscriptionBuilder()
  .subscribe([
    tables.myLobbyTickets,
    tables.myLobbyRooms,
    tables.myLobbyRoomSeats,
  ]);
```

`findDuel` is the example's product-facing wrapper. A host can instead expose
its own reducer around `lobby.joinQueue(ctx.as.lobby, ...)`.

### Publish Lobby as the database

The root entrypoint includes the standalone lifecycle hook for databases
dedicated to Lobby:

```ts
export { default, init } from '@spacetimedb/lobby';
export {
  join_queue,
  join_ranked_queue,
  cancel_ticket,
  myLobbyTickets,
  myLobbyRooms,
  lobbyQueueSummary,
  lobbyRankedLeaderboard,
} from '@spacetimedb/lobby';
```

## API

Reducers:

- `join_queue({ pool, matchSize, attributesJson, ttlSeconds})`
- `join_ranked_queue({ pool, matchSize, attributesJson, ttlSeconds, ratingPool })`
- `cancel_ticket({ ticketId })`
- `join_room({ roomId })`
- `leave_room({ roomId })`
- `close_room({ roomId })`
- `expire_tickets({ limit })` (admin only, up to 1,000 rows)
- `set_rating({ pool, subject, rating })` (admin only)
- `update_config({ defaultTicketTtlSeconds, maxMatchSize })` (admin only)
- `add_admin_identity({ identity })` (admin only)
- `remove_admin_identity({ identity })` (admin only)

Procedure:

- `get_lobby_status()` returns a JSON string.

Views:

- `my_lobby_tickets`
- `my_lobby_ratings`
- `my_lobby_rooms`
- `my_lobby_room_seats`
- `lobby_queue_summary`
- `lobby_ranked_leaderboard`
- `lobby_admin_tickets`
- `lobby_admin_rooms`
- `lobby_admin_room_seats`
- `lobby_admin_match_results`

Host helper API:

- Queue lifecycle: `joinQueue`, `joinRankedQueue`, and `cancelTicket`.
- Room lifecycle: `joinRoom`, `leaveRoom`, and `closeRoom`.
- Ranking: `reportMatchResult`.

Submodule administrator operations include `set_rating`, `expire_tickets`, and
`update_config`.

Package entrypoints:

- `@spacetimedb/lobby` can run as a standalone Lobby database.
- `@spacetimedb/lobby/submodule` supplies the submodule namespace and host
  helpers.

## Matching

Matching is deterministic:

- tickets match only within the same `pool`
- tickets match only with the same `matchSize`
- an indexed `(pool, status, createdAt)` scan selects the oldest eligible tickets
- matched tickets create one room and one reserved seat per ticket
- rooms become `Active` when every reserved seat joins

`attributesJson` stores host-defined matching metadata. Host wrappers interpret
the metadata when applying product-specific rules.

Ranked queues use a 1,000 starting rating and a widening rating band: 100
points initially, 50 more for each 10 seconds waited, capped at 800. A host
host reports a two-player result through `reportMatchResult` after validating
its game-specific completion rules. Results are idempotent per room and update
both players with Elo K=32. The room must be active. Result reporting is a host
helper and is absent from the generic client-callable API.

Any participant may close a room through `close_room`. Hosts that need stricter
completion rules should expose their own reducer and call `closeRoom` after
validating the game state.

## Testing

```bash
pnpm test
pnpm run typecheck
pnpm run build
```

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
