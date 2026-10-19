import * as v from 'valibot';
import type {
  ProcedureExport,
  OptionBuilder,
  ArrayBuilder,
} from 'spacetimedb/server';
import {
  EmailStatus,
  emailStatus,
  resendDeliveryEventTable,
  resendEmailTable,
  resendWebhookEventTable,
  sendEmailResult,
  spacetimedb,
  t,
  unitResult,
  vSendEmailResponse,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema.js';
import { callResend, ensureOkOrThrow } from './http.js';
import {
  safeJsonParse,
  summarizeIssues,
  throwSenderError,
} from './validation.js';
import { upsertEmail } from './email_writes.js';
import { loadConfigOrThrowFromProcedure } from './config.js';
import { adminVerdict, denyIfNotAdmin } from './auth.js';
import { validateEmailInput } from './email-input.js';
import { errors } from './errors.js';

function requireProcedureAdmin(ctx: ProcedureModuleCtx): void {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
}

const MAX_QUERY_ROWS = 1000;

function takeRows<T>(rows: Iterable<T>): T[] {
  const out: T[] = [];
  for (const row of rows) {
    if (out.length >= MAX_QUERY_ROWS) break;
    out.push(row);
  }
  return out;
}

// Resend's send API takes tags as `[{ name, value }]`.
const vSendTags = v.array(v.object({ name: v.string(), value: v.string() }));

function extractTagFieldsFromJson(tagsJson: string | undefined): {
  userId: string | undefined;
  orgId: string | undefined;
} {
  const result = v.safeParse(vSendTags, tagsJson && safeJsonParse(tagsJson));
  if (!result.success) return { userId: undefined, orgId: undefined };
  const tags = result.output;
  return {
    userId: tags.find(tag => tag.name === 'userId')?.value,
    orgId: tags.find(tag => tag.name === 'orgId')?.value,
  };
}

// Resend expects snake_case fields in the POST /emails request body.
type ResendSendEmailBody = {
  from: string;
  to: string[];
  subject: string;
  html?: string;
  text?: string;
  cc?: string[];
  bcc?: string[];
  reply_to?: string[];
  scheduled_at?: string;
  tags?: unknown;
  headers?: unknown;
};

function buildSendEmailBody(args: {
  from: string;
  to: string[];
  subject: string;
  html: string | undefined;
  text: string | undefined;
  cc: string[] | undefined;
  bcc: string[] | undefined;
  replyTo: string[] | undefined;
  tagsJson: string | undefined;
  headersJson: string | undefined;
  scheduledAt: string | undefined;
}): string {
  const body: ResendSendEmailBody = {
    from: args.from,
    to: args.to,
    subject: args.subject,
  };
  if (args.html !== undefined) body.html = args.html;
  if (args.text !== undefined) body.text = args.text;
  if (args.cc !== undefined && args.cc.length > 0) body.cc = args.cc;
  if (args.bcc !== undefined && args.bcc.length > 0) body.bcc = args.bcc;
  if (args.replyTo !== undefined && args.replyTo.length > 0) {
    body.reply_to = args.replyTo;
  }
  if (args.scheduledAt !== undefined) body.scheduled_at = args.scheduledAt;
  if (args.tagsJson !== undefined) {
    const parsed = safeJsonParse(args.tagsJson);
    if (parsed !== undefined) body.tags = parsed;
  }
  if (args.headersJson !== undefined) {
    const parsed = safeJsonParse(args.headersJson);
    if (parsed !== undefined) body.headers = parsed;
  }
  return JSON.stringify(body);
}

function recordQueuedEmail(
  ctx: WriteCtx,
  now: ProcedureModuleCtx['timestamp'],
  args: {
    resendId: string;
    from: string;
    to: string[];
    subject: string;
    html: string | undefined;
    text: string | undefined;
    tagsJson: string | undefined;
  }
) {
  const tagFields = extractTagFieldsFromJson(args.tagsJson);
  upsertEmail(ctx, now, {
    resendId: args.resendId,
    fromAddress: args.from,
    toAddressesJson: JSON.stringify(args.to),
    subject: args.subject,
    html: args.html,
    text: args.text,
    status: EmailStatus.Queued,
    lastError: undefined,
    bouncedAt: undefined,
    bounceJson: undefined,
    failedAt: undefined,
    failureReason: undefined,
    complained: false,
    complainedAt: undefined,
    opened: false,
    openedAt: undefined,
    clicked: false,
    clickedAt: undefined,
    deliveredAt: undefined,
    sentAt: undefined,
    tagsJson: args.tagsJson,
    userId: tagFields.userId,
    orgId: tagFields.orgId,
  });
}

export type SendEmailArgs = {
  from?: string | undefined;
  to: string[];
  subject: string;
  html?: string | undefined;
  text?: string | undefined;
  cc?: string[] | undefined;
  bcc?: string[] | undefined;
  replyTo?: string[] | undefined;
  tagsJson?: string | undefined;
  headersJson?: string | undefined;
  scheduledAt?: string | undefined;
  idempotencyKey?: string | undefined;
};

export function sendEmailRequest(ctx: ProcedureModuleCtx, args: SendEmailArgs) {
  try {
    validateEmailInput(args);
  } catch (error) {
    throwSenderError(
      error instanceof Error ? error.message : errors.sendEmailInvalidInput
    );
  }
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const fromAddress = args.from ?? cfg.defaultFrom;
  if (!fromAddress) throwSenderError(errors.sendEmailMissingFrom);

  const jsonBody = buildSendEmailBody({
    from: fromAddress,
    to: args.to,
    subject: args.subject,
    html: args.html,
    text: args.text,
    cc: args.cc,
    bcc: args.bcc,
    replyTo: args.replyTo,
    tagsJson: args.tagsJson,
    headersJson: args.headersJson,
    scheduledAt: args.scheduledAt,
  });

  const response = callResend(ctx, {
    method: 'POST',
    path: '/emails',
    apiKey: cfg.apiKey,
    jsonBody,
    idempotencyKey: args.idempotencyKey,
  });
  ensureOkOrThrow(response, errors.sendEmailFailed);

  const parsed = safeJsonParse(response.body);
  if (parsed === undefined) throwSenderError(errors.sendEmailInvalidResponse);
  const result = v.safeParse(vSendEmailResponse, parsed);
  if (!result.success) {
    throwSenderError(
      `${errors.sendEmailInvalidResponse}:${summarizeIssues(result.issues)}`
    );
  }

  const resendId = result.output.id;
  ctx.withTx(tx => {
    recordQueuedEmail(tx, ctx.timestamp, {
      resendId,
      from: fromAddress,
      to: args.to,
      subject: args.subject,
      html: args.html,
      text: args.text,
      tagsJson: args.tagsJson,
    });
  });
  return { resendId };
}

const sendEmailArgs = {
  from: t.option(t.string()),
  to: t.array(t.string()),
  subject: t.string(),
  html: t.option(t.string()),
  text: t.option(t.string()),
  cc: t.option(t.array(t.string())),
  bcc: t.option(t.array(t.string())),
  replyTo: t.option(t.array(t.string())),
  tagsJson: t.option(t.string()),
  headersJson: t.option(t.string()),
  scheduledAt: t.option(t.string()),
  idempotencyKey: t.option(t.string()),
};

export const sendEmail: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof sendEmailArgs,
  typeof sendEmailResult
> = spacetimedb.procedure(sendEmailArgs, sendEmailResult, (ctx, args) => {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
  return sendEmailRequest(ctx, args);
});

const cancelEmailArgs = { resendId: t.string() };
export const cancelEmail: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof cancelEmailArgs,
  typeof unitResult
> = spacetimedb.procedure(cancelEmailArgs, unitResult, (ctx, args) => {
  const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
  denyIfNotAdmin(verdict);
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const response = callResend(ctx, {
    method: 'POST',
    path: `/emails/${args.resendId}/cancel`,
    apiKey: cfg.apiKey,
    jsonBody: undefined,
    idempotencyKey: undefined,
  });
  ensureOkOrThrow(response, errors.cancelEmailFailed);

  ctx.withTx(tx => {
    const existing = tx.db.resendEmail.resendId.find(args.resendId);
    if (!existing) return;
    const updated = {
      ...existing,
      status: EmailStatus.Cancelled,
      statusUpdatedAt: ctx.timestamp,
      updatedAt: ctx.timestamp,
    };
    tx.db.resendEmail.resendId.update(updated);
  });
  return {};
});

const getEmailArgs = { resendId: t.string() };
const getEmailResult: OptionBuilder<typeof resendEmailTable.rowType> = t.option(
  resendEmailTable.rowType
);
export const getEmail: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getEmailArgs,
  typeof getEmailResult
> = spacetimedb.procedure(getEmailArgs, getEmailResult, (ctx, { resendId }) => {
  requireProcedureAdmin(ctx);
  return ctx.withTx(
    tx => tx.db.resendEmail.resendId.find(resendId) ?? undefined
  );
});

const getWebhookEventArgs = { eventId: t.string() };
const getWebhookEventResult: OptionBuilder<
  typeof resendWebhookEventTable.rowType
> = t.option(resendWebhookEventTable.rowType);
export const getWebhookEvent: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof getWebhookEventArgs,
  typeof getWebhookEventResult
> = spacetimedb.procedure(
  getWebhookEventArgs,
  getWebhookEventResult,
  (ctx, { eventId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(
      tx => tx.db.resendWebhookEvent.eventId.find(eventId) ?? undefined
    );
  }
);

const emailListResult: ArrayBuilder<typeof resendEmailTable.rowType> = t.array(
  resendEmailTable.rowType
);

const listEmailsByUserIdArgs = { userId: t.string() };
export const listEmailsByUserId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listEmailsByUserIdArgs,
  typeof emailListResult
> = spacetimedb.procedure(
  listEmailsByUserIdArgs,
  emailListResult,
  (ctx, { userId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx =>
      takeRows(tx.db.resendEmail.byUserId.filter(userId))
    );
  }
);

const listEmailsByOrgIdArgs = { orgId: t.string() };
export const listEmailsByOrgId: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listEmailsByOrgIdArgs,
  typeof emailListResult
> = spacetimedb.procedure(
  listEmailsByOrgIdArgs,
  emailListResult,
  (ctx, { orgId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx => takeRows(tx.db.resendEmail.byOrgId.filter(orgId)));
  }
);

const listEmailsByStatusArgs: { status: typeof emailStatus } = {
  status: emailStatus,
};
export const listEmailsByStatus: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listEmailsByStatusArgs,
  typeof emailListResult
> = spacetimedb.procedure(
  listEmailsByStatusArgs,
  emailListResult,
  (ctx, { status }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx =>
      takeRows(tx.db.resendEmail.byStatus.filter(status))
    );
  }
);

const listDeliveryEventsForEmailArgs = { resendId: t.string() };
const listDeliveryEventsForEmailResult: ArrayBuilder<
  typeof resendDeliveryEventTable.rowType
> = t.array(resendDeliveryEventTable.rowType);
export const listDeliveryEventsForEmail: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof listDeliveryEventsForEmailArgs,
  typeof listDeliveryEventsForEmailResult
> = spacetimedb.procedure(
  listDeliveryEventsForEmailArgs,
  listDeliveryEventsForEmailResult,
  (ctx, { resendId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx =>
      takeRows(tx.db.resendDeliveryEvent.byResendId.filter(resendId))
    );
  }
);

const resendApiRequestArgs = {
  method: t.string(),
  path: t.string(),
  jsonBody: t.option(t.string()),
  idempotencyKey: t.option(t.string()),
};
const resendApiRequestResult = t.object('ResendApiRequestResult', {
  status: t.u16(),
  body: t.string(),
});
export const resendApiRequest: ProcedureExport<
  typeof spacetimedb.schemaType,
  typeof resendApiRequestArgs,
  typeof resendApiRequestResult
> = spacetimedb.procedure(
  resendApiRequestArgs,
  resendApiRequestResult,
  (ctx, args) => {
    // Administrators may make authenticated Resend calls with the stored key.
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    const cfg = loadConfigOrThrowFromProcedure(ctx);
    return callResend(ctx, {
      method: args.method,
      path: args.path,
      apiKey: cfg.apiKey,
      jsonBody: args.jsonBody,
      idempotencyKey: args.idempotencyKey,
    });
  }
);
