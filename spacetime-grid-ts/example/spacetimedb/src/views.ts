import { t, type ViewCtx } from 'spacetimedb/server';
import * as gridSubmodule from '@spacetimedb/grid/submodule';

import {
  authUserViewRow,
  MatchStatus,
  match,
  matchParticipant,
  playerUnit,
  spacetimedb,
  type Schema,
} from './schema';
export { default } from './schema';

export const myAuthUser = spacetimedb.view(
  { name: 'my_auth_user', public: true },
  t.array(authUserViewRow),
  ctx => {
    const binding = ctx.db.auth.authConnectionBinding.stdbIdentity.find(
      ctx.sender
    );
    if (!binding) return [];
    const row = ctx.db.auth.authUser.userId.find(binding.userId);
    return row ? [row] : [];
  }
);

// Per-match scoping. Caller sees matches they participate in, the grid
// state for those matches, and other seats' participant rows so the lobby
// can render opponent names. Other matches are invisible.
function callerUserId(ctx: ViewCtx<Schema>): string | undefined {
  return ctx.db.auth.authConnectionBinding.stdbIdentity.find(ctx.sender)
    ?.userId;
}

function myMatchAndGridIds(ctx: ViewCtx<Schema>): {
  matchIds: Set<bigint>;
  gridIds: Set<bigint>;
} {
  const matchIds = new Set<bigint>();
  const gridIds = new Set<bigint>();
  const uid = callerUserId(ctx);
  if (!uid) return { matchIds, gridIds };
  for (const p of ctx.db.matchParticipant.userId.filter(uid)) {
    if (matchIds.has(p.matchId)) continue;
    matchIds.add(p.matchId);
    const m = ctx.db.match.matchId.find(p.matchId);
    if (m) gridIds.add(m.gridId);
  }
  return { matchIds, gridIds };
}

export const myMatches = spacetimedb.view(
  { name: 'my_matches', public: true },
  t.array(match.rowType),
  ctx => {
    const { matchIds } = myMatchAndGridIds(ctx);
    if (matchIds.size === 0) return [];
    const out = [];
    for (const id of matchIds) {
      const m = ctx.db.match.matchId.find(id);
      if (m) out.push(m);
    }
    return out;
  }
);
export const myGrids = spacetimedb.view(
  { name: 'my_grids', public: true },
  t.array(gridSubmodule.grid.rowType),
  ctx => {
    const { gridIds } = myMatchAndGridIds(ctx);
    if (gridIds.size === 0) return [];
    const out = [];
    for (const id of gridIds) {
      const g = ctx.db.grid.grid.id.find(id);
      if (g) out.push(g);
    }
    return out;
  }
);

export const myCellStates = spacetimedb.view(
  { name: 'my_cell_states', public: true },
  t.array(gridSubmodule.cellState.rowType),
  ctx => {
    const { gridIds } = myMatchAndGridIds(ctx);
    if (gridIds.size === 0) return [];
    const out = [];
    for (const gid of gridIds) {
      for (const c of ctx.db.grid.cellState.gridId.filter(gid)) out.push(c);
    }
    return out;
  }
);

export const myGridEntities = spacetimedb.view(
  { name: 'my_grid_entities', public: true },
  t.array(gridSubmodule.gridEntity.rowType),
  ctx => {
    const { gridIds } = myMatchAndGridIds(ctx);
    if (gridIds.size === 0) return [];
    const out = [];
    for (const gid of gridIds) {
      for (const e of ctx.db.grid.gridEntity.gridId.filter(gid)) out.push(e);
    }
    return out;
  }
);

export const myPlayerUnits = spacetimedb.view(
  { name: 'my_player_units', public: true },
  t.array(playerUnit.rowType),
  ctx => {
    const { matchIds } = myMatchAndGridIds(ctx);
    if (matchIds.size === 0) return [];
    const out = [];
    for (const mid of matchIds) {
      for (const u of ctx.db.playerUnit.matchId.filter(mid)) out.push(u);
    }
    return out;
  }
);

export const myMatchParticipants = spacetimedb.view(
  { name: 'my_match_participants', public: true },
  t.array(matchParticipant.rowType),
  ctx => {
    const { matchIds } = myMatchAndGridIds(ctx);
    if (matchIds.size === 0) return [];
    const out = [];
    for (const mid of matchIds) {
      for (const p of ctx.db.matchParticipant.matchId.filter(mid)) out.push(p);
    }
    return out;
  }
);

// Discriminator for actor_directory rows. User = real auth_user; Npc =
// npc_actor row (built-in opponents).
const actorKind = t.enum('ActorKind', ['User', 'Npc']);
const ActorKind = {
  User: { tag: 'User' as const },
  Npc: { tag: 'Npc' as const },
};

function visibleActorIds(ctx: ViewCtx<Schema>): Set<string> {
  const ids = new Set<string>();
  const uid = callerUserId(ctx);
  if (!uid) return ids;
  ids.add(uid);

  for (const ownSeat of ctx.db.matchParticipant.userId.filter(uid)) {
    for (const seat of ctx.db.matchParticipant.matchId.filter(
      ownSeat.matchId
    )) {
      ids.add(seat.userId);
    }
  }

  for (const pending of ctx.db.match.status.filter(MatchStatus.Waiting)) {
    for (const seat of ctx.db.matchParticipant.matchId.filter(
      pending.matchId
    )) {
      if (seat.seatIdx === 0) ids.add(seat.userId);
    }
  }
  return ids;
}

// Safe profile fields for actors the caller can currently encounter: the
// caller, participants in their matches, and hosts of joinable matches.
export const actorDirectory = spacetimedb.view(
  { name: 'actor_directory', public: true },
  t.array(
    t.object('ActorDirectoryRow', {
      actorId: t.string(),
      name: t.option(t.string()),
      image: t.option(t.string()),
      kind: actorKind,
    })
  ),
  ctx => {
    const out = [];
    for (const actorId of visibleActorIds(ctx)) {
      const user = ctx.db.auth.authUser.userId.find(actorId);
      if (user) {
        out.push({
          actorId: user.userId,
          name: user.name,
          image: user.image,
          kind: ActorKind.User,
        });
        continue;
      }
      const npc = ctx.db.npcActor.actorId.find(actorId);
      if (npc) {
        out.push({
          actorId: npc.actorId,
          name: npc.name,
          image: npc.image,
          kind: ActorKind.Npc,
        });
      }
    }
    return out;
  }
);

// Public discovery of joinable matches. Anyone signed in can see Waiting
// matches and call join_match.
export const lobbyOpenMatches = spacetimedb.view(
  { name: 'lobby_open_matches', public: true },
  t.array(
    t.object('LobbyOpenMatch', {
      matchId: t.u64(),
      hostUserId: t.string(),
      createdAt: t.timestamp(),
    })
  ),
  ctx => {
    const out = [];
    for (const m of ctx.db.match.status.filter(MatchStatus.Waiting)) {
      const seat0 = [...ctx.db.matchParticipant.matchId.filter(m.matchId)].find(
        p => p.seatIdx === 0
      );
      if (!seat0) continue;
      out.push({
        matchId: m.matchId,
        hostUserId: seat0.userId,
        createdAt: m.createdAt,
      });
    }
    return out;
  }
);
