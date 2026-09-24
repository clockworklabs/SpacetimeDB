import {
  schema,
  t,
  table,
  type InferSchema,
  type TransactionCtx,
} from 'spacetimedb/server';
import * as auth from '@spacetimedb/auth/submodule';
import * as gridSubmodule from '@spacetimedb/grid/submodule';
import { type SendMailFn, type MailParams } from '@spacetimedb/auth/submodule';

export const consoleSendMail: SendMailFn = (_ctx, params: MailParams) => {
  console.log(
    `[mail] to=${params.to} subject=${params.subject}\n${params.text}`
  );
};

export const authUserViewRow = t.object('GridAuthUser', {
  userId: t.string(),
  email: t.string(),
  emailVerified: t.bool(),
  name: t.option(t.string()),
  image: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
});

// Static unit catalog. Seeded once in init.
export const unitType = table(
  { name: 'unit_type', public: true },
  {
    typeId: t.string().primaryKey(),
    name: t.string(),
    movement: t.i32(),
    attackRange: t.i32(),
    attackDmg: t.i32(),
    hp: t.i32(),
    glyph: t.string(),
  }
);

// Match lifecycle. Waiting = seats not yet filled. Active = playing.
// Ended = a side has won (winnerUserId is set).
export const matchStatus = t.enum('MatchStatus', [
  'Waiting',
  'Active',
  'Ended',
]);
export const MatchStatus = {
  Waiting: { tag: 'Waiting' as const },
  Active: { tag: 'Active' as const },
  Ended: { tag: 'Ended' as const },
};

// One row per match. Participant userIds live in match_participant, not here.
export const match = table(
  { name: 'match', public: false },
  {
    matchId: t.u64().primaryKey().autoInc(),
    status: matchStatus.index(),
    currentSeatIdx: t.i32(),
    turnNumber: t.i32(),
    winnerUserId: t.option(t.string()),
    gridId: t.u64().index(),
    createdAt: t.timestamp(),
    updatedAt: t.timestamp(),
  }
);

// Seats in a match. matchId+userId pair via two indexes  -  no host/opponent
// asymmetry, naturally extends past 2 seats, and a player's matches are
// reachable through matchParticipant.userId in O(log n). team groups allies
// for win-check semantics; in free-for-all each seat is its own team.
export const matchParticipant = table(
  { name: 'match_participant', public: false },
  {
    id: t.u64().primaryKey().autoInc(),
    matchId: t.u64().index(),
    userId: t.string().index(),
    seatIdx: t.i32(),
    team: t.i32(),
    joinedAt: t.timestamp(),
  }
);

// Non-player actors include the built-in AI. Their actorId shares the
// auth_user.userId namespace for ownership columns. A separate table keeps
// NPCs out of authentication and user-directory rows.
export const npcActor = table(
  { name: 'npc_actor', public: true },
  {
    actorId: t.string().primaryKey(),
    name: t.string(),
    image: t.option(t.string()),
    createdAt: t.timestamp(),
  }
);

export const AI_BOT_USER_ID = 'ai-bot-001';
export const AI_BOT_NAME = 'Xeno Garrison';

// Combat state per unit. The grid submodule owns positional state
// (gridEntity.x/y); this row holds the game-specific layer on top.
export const playerUnit = table(
  { name: 'player_unit', public: false },
  {
    entityId: t.u64().primaryKey(), // FK into grid_entity.id
    matchId: t.u64().index(),
    ownerUserId: t.string().index(),
    typeId: t.string().index(),
    currentHp: t.i32(),
    hasMoved: t.bool(),
    hasAttacked: t.bool(),
    createdAt: t.timestamp(),
  }
);

export const spacetimedb = schema({
  auth,
  grid: gridSubmodule,
  unitType,
  match,
  matchParticipant,
  npcActor,
  playerUnit,
});
export default spacetimedb;

export type Schema = InferSchema<typeof spacetimedb>;
export type WriteCtx = TransactionCtx<Schema>;
