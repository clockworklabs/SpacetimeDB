import { SenderError } from 'spacetimedb/server';
import { errors } from '../errors.js';
import type { ViewModuleCtx, WriteCtx } from './schema.js';

export function isAdmin(ctx: WriteCtx | ViewModuleCtx): boolean {
  return ctx.db.apiKeyAdminIdentity.identity.find(ctx.sender) != null;
}

export function requireAdmin(ctx: WriteCtx): void {
  if (!isAdmin(ctx)) throw new SenderError(errors.notAuthorized);
}
