import type {
  ReducerModuleCtx,
  TransactionModuleCtx,
  ViewModuleCtx,
} from './schema';
import { SenderError } from './schema';

type AdminCtx = ReducerModuleCtx | TransactionModuleCtx | ViewModuleCtx;

export function isAdmin(ctx: AdminCtx, sender = ctx.sender): boolean {
  return ctx.db.apiKeyAdminIdentity.identity.find(sender) != null;
}

export function requireAdmin(
  ctx: ReducerModuleCtx | TransactionModuleCtx,
  sender = ctx.sender
): void {
  if (!isAdmin(ctx, sender)) throw new SenderError('api_keys.not_authorized');
}
