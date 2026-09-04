import {
  EmailStatus,
  type EmailStatusValue,
  type ModuleTimestamp,
  type WriteCtx,
} from './schema';

// Any field passed as `undefined` preserves the existing row's value. Used by webhooks (sparse per-event fields).
export function upsertEmail(
  ctx: WriteCtx,
  now: ModuleTimestamp,
  args: {
    resendId: string;
    fromAddress: string;
    toAddressesJson: string;
    subject: string | undefined;
    html: string | undefined;
    text: string | undefined;
    status: EmailStatusValue | undefined;
    lastError: string | undefined;
    bouncedAt: ModuleTimestamp | undefined;
    bounceJson: string | undefined;
    failedAt: ModuleTimestamp | undefined;
    failureReason: string | undefined;
    complained: boolean;
    complainedAt: ModuleTimestamp | undefined;
    opened: boolean;
    openedAt: ModuleTimestamp | undefined;
    clicked: boolean;
    clickedAt: ModuleTimestamp | undefined;
    deliveredAt: ModuleTimestamp | undefined;
    sentAt: ModuleTimestamp | undefined;
    tagsJson: string | undefined;
    userId: string | undefined;
    orgId: string | undefined;
  }
) {
  const existing = ctx.db.resendEmail.resendId.find(args.resendId);
  const row = {
    resendId: args.resendId,
    fromAddress: args.fromAddress,
    toAddressesJson: args.toAddressesJson,
    subject: args.subject ?? existing?.subject,
    html: args.html ?? existing?.html,
    text: args.text ?? existing?.text,
    status: args.status ?? existing?.status ?? EmailStatus.Queued,
    lastError: args.lastError ?? existing?.lastError,
    bouncedAt: args.bouncedAt ?? existing?.bouncedAt,
    bounceJson: args.bounceJson ?? existing?.bounceJson,
    failedAt: args.failedAt ?? existing?.failedAt,
    failureReason: args.failureReason ?? existing?.failureReason,
    complained: args.complained || (existing?.complained ?? false),
    complainedAt: args.complainedAt ?? existing?.complainedAt,
    opened: args.opened || (existing?.opened ?? false),
    openedAt: args.openedAt ?? existing?.openedAt,
    clicked: args.clicked || (existing?.clicked ?? false),
    clickedAt: args.clickedAt ?? existing?.clickedAt,
    deliveredAt: args.deliveredAt ?? existing?.deliveredAt,
    sentAt: args.sentAt ?? existing?.sentAt,
    tagsJson: args.tagsJson ?? existing?.tagsJson,
    userId: args.userId ?? existing?.userId,
    orgId: args.orgId ?? existing?.orgId,
    createdAt: existing?.createdAt ?? now,
    updatedAt: now,
  };

  if (!existing) {
    ctx.db.resendEmail.insert(row);
    return;
  }
  if (ctx.db.resendEmail.resendId.update) {
    ctx.db.resendEmail.resendId.update(row);
  } else {
    ctx.db.resendEmail.delete(existing);
    ctx.db.resendEmail.insert(row);
  }
}
