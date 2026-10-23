import type { WriteCtx } from './schema';
import { throwSenderError } from './validation';

// The Stripe submodule's admin list is the only admin list. Its `install` seeds
// the publishing identity and `stripe.add_admin_identity` grants others.
export function requireAdmin(ctx: WriteCtx): void {
  if (ctx.db.stripe.stripeAdminIdentity.identity.find(ctx.sender) == null) {
    throwSenderError('store.not_authorized');
  }
}
