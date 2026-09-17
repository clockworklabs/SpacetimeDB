import type { ReducerModuleCtx } from './schema';

const DEFAULT_TICKET_TTL_SECONDS = 60;
const DEFAULT_MAX_MATCH_SIZE = 16;

export function installLobby(ctx: ReducerModuleCtx) {
  if (ctx.db.lobbyConfig.singleton.find(true) == null) {
    ctx.db.lobbyConfig.insert({
      singleton: true,
      defaultTicketTtlSeconds: DEFAULT_TICKET_TTL_SECONDS,
      maxMatchSize: DEFAULT_MAX_MATCH_SIZE,
      updatedAt: ctx.timestamp,
    });
  }

  if (ctx.db.lobbyAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.lobbyAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
}
