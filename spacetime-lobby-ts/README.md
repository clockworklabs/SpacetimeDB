# @spacetimedb/lobby

Match players and manage game rooms in your SpacetimeDB application. Players
join a queue, receive a place in a room, and join the match when everyone is
ready. Ranked two-player matches can update player ratings from the result
your game reports.

The submodule tracks waiting players and room state, and cleans up expired
queue entries and abandoned rooms. Your game decides its match rules and who
won.

## Install

```bash
npm install @spacetimedb/lobby spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

## Integrate into an application

Add Lobby to your module and initialize its matchmaking settings and cleanup:

```ts
import { schema } from 'spacetimedb/server';
import * as lobby from '@spacetimedb/lobby/submodule';

const spacetimedb = schema({ lobby });

export const init = spacetimedb.init(ctx => {
  lobby.install(ctx.as.lobby);
});

export default spacetimedb;
```

`install` seeds the default config, makes the publishing identity the first
lobby administrator, and schedules the cleanup sweep.

Players can queue, cancel, join, and leave through the submodule's own reducers,
which identify them with `ctx.sender.toHexString()`. For ranked matches, your
game chooses the rating pool and reports the result through server-side helpers.

This example queues a player for a ranked duel. Replace `userIdFor` with your
application's authenticated user lookup:

```ts
export const findDuel = spacetimedb.reducer({}, ctx => {
  lobby.joinRankedQueueForSubject(ctx.as.lobby, {
    pool: 'duel',
    subject: userIdFor(ctx),
    matchSize: 2,
    attributesJson: JSON.stringify({ region: 'iad' }),
  });
});
```

See the [Starclash host module](./example/spacetimedb/) for profile mapping,
matchmaking, match results, and caller-scoped views.

After generating bindings, a client joins through the host operation and reads
match state through subscriptions:

```ts
import { tables } from './module_bindings';

await conn.reducers.findDuel({});

conn
  .subscriptionBuilder()
  .subscribe([
    tables.lobby.myLobbyTickets,
    tables.lobby.myLobbyRooms,
    tables.lobby.myLobbyRoomSeats,
  ]);
```

### Publish Lobby as the database

[`spacetimedb/`](./spacetimedb/) publishes the submodule as a standalone
database with its client reducers, views, and administrator operations.

## API

Client reducers:

- `join_queue({ pool, matchSize, attributesJson, ttlSeconds })` queues the
  caller for an unranked match. A subject waits in one queue at a time, so
  joining replaces its queued ticket.
- `cancel_ticket({ ticketId })`
- `join_room({ roomId })` marks the caller's reserved seat joined.
- `leave_room({ roomId })` marks the caller's seat left. Leaving a `Ready` room
  abandons it. An `Active` room stays active, so its result can still be
  reported, until every seat has left.

Administrator reducers:

- `set_rating({ pool, subject, rating })`
- `update_config({ defaultTicketTtlSeconds, maxMatchSize, readyTimeoutSeconds, retentionSeconds })`
- `add_lobby_admin({ identity })` and `remove_lobby_admin({ identity })`. The
  last administrator cannot be removed.

Procedure:

- `get_lobby_status()` returns the configured defaults and queued, ready, and
  active counts.

Views:

- `my_lobby_tickets`
- `my_lobby_ratings`
- `my_lobby_rooms`
- `my_lobby_room_seats`
- `lobby_queue_summary`
- `lobby_ranked_leaderboard`: up to 500 ratings ordered by rating pool, then
  rating.
- `lobby_admin_tickets`
- `lobby_admin_rooms`
- `lobby_admin_room_seats`
- `lobby_admin_match_results`

Host helpers, called with `ctx.as.lobby` or `tx.as.lobby`:

- `joinQueueForSubject`, `joinRankedQueueForSubject`, and
  `cancelTicketForSubject`.
- `joinRoomForSubject` and `leaveRoomForSubject`.
- `reportMatchResult({ roomId, winnerSubject })` records the result of an
  active two-player room, updates both ratings, and closes the room. Omit
  `winnerSubject` for a draw.
- `closeRoom(tx, roomId)` closes a room without a result.

Every failure throws a code from the exported `errors` object.

Package entrypoints:

- `@spacetimedb/lobby` exports `errors` and the host helpers.
- `@spacetimedb/lobby/submodule` supplies the submodule namespace, `install`,
  host helpers, reducers, and views.

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
points initially, 50 more for each 10 seconds waited, capped at 800. The oldest
ticket that can fill a match takes the closest ratings in its rating pool. A
host reports a two-player result through `reportMatchResult` after validating
its game-specific completion rules; seats that left still receive the result.
Results update both players with Elo K=32.

## Cleanup

A scheduled sweep runs every 15 seconds and handles up to 500 rows per step:

- queued tickets past their expiry become `Expired`
- `Ready` rooms whose seats are not all joined within `readyTimeoutSeconds`
  (default 120) become `Abandoned`
- cancelled and expired tickets, closed and abandoned rooms with their seats
  and tickets, and match results are deleted after `retentionSeconds`
  (default one hour)

## Testing

```bash
pnpm test
pnpm run typecheck
```

## License

[Apache-2.0](./LICENSE.txt).
