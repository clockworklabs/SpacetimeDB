import { DEFAULT_SWEEP_BATCH, installRateLimitState } from '../index';
import type { ReducerModuleCtx } from './schema';

export function installRateLimit(ctx: ReducerModuleCtx) {
  if (ctx.db.rateLimitAdminIdentity.identity.find(ctx.sender) == null) {
    ctx.db.rateLimitAdminIdentity.insert({
      identity: ctx.sender,
      addedAtMicros: ctx.timestamp.microsSinceUnixEpoch,
    });
  }
  installRateLimitState(ctx, { sweepBatch: DEFAULT_SWEEP_BATCH });
}
