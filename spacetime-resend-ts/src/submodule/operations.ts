import * as v from 'valibot';
import {
  EmailStatus,
  emailStatus,
  resendDeliveryEventTable,
  resendEmailTable,
  sendEmailResult,
  spacetimedb,
  t,
  vSendEmailResponse,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema';
import { callResend, ensureOkOrThrow } from './http';
import {
  parseWithSchema,
  safeJsonParse,
  summarizeIssues,
  throwSenderError,
} from './validation';
import { upsertEmail } from './email_writes';
import { loadConfigOrThrowFromProcedure } from './config';
import { adminVerdict, denyIfNotAdmin } from './auth';
import { validateEmailInput } from './email-input';

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

const vTagsForExtraction = v.union([
  v.record(v.string(), v.string()),
  v.array(v.object({ name: v.string(), value: v.string() })),
]);

function extractTagFieldsFromJson(tagsJson: string | undefined): {
  userId: string | undefined;
  orgId: string | undefined;
} {
  if (!tagsJson) return { userId: undefined, orgId: undefined };
  const parsed = safeJsonParse(tagsJson);
  if (parsed === undefined) return { userId: undefined, orgId: undefined };
  const result = parseWithSchema(vTagsForExtraction, parsed);
  if (result.kind === 'error') {
    return { userId: undefined, orgId: undefined };
  }
  const tags = result.data;
  if (Array.isArray(tags)) {
    let userId: string | undefined;
    let orgId: string | undefined;
    for (const tag of tags) {
      if (tag.name === 'userId') userId = tag.value;
      if (tag.name === 'orgId') orgId = tag.value;
    }
    return { userId, orgId };
  }
  return { userId: tags['userId'], orgId: tags['orgId'] };
}

// Build POST /emails body. Resend HTTP API expects snake_case on the wire; SDK converts internally. We hand-roll, so emit snake_case directly.
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

export function sendEmail(ctx: ProcedureModuleCtx, args: SendEmailArgs) {
  try {
    validateEmailInput(args);
  } catch (error) {
    throwSenderError(
      error instanceof Error ? error.message : 'resend.send_email_invalid_input'
    );
  }
  const cfg = loadConfigOrThrowFromProcedure(ctx);
  const fromAddress = args.from ?? cfg.defaultFrom;
  if (!fromAddress) throwSenderError('resend.send_email_missing_from');

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
  ensureOkOrThrow(response, 'resend.send_email_failed');

  const parsed = safeJsonParse(response.body);
  if (parsed === undefined)
    throwSenderError('resend.send_email_invalid_response');
  const result = parseWithSchema(vSendEmailResponse, parsed);
  if (result.kind === 'error') {
    throwSenderError(
      `resend.send_email_invalid_response:${summarizeIssues(result.issues)}`
    );
  }

  const resendId = result.data.id;
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

export const send_email = spacetimedb.procedure(
  sendEmailArgs,
  sendEmailResult,
  (ctx, args) => {
    const verdict = ctx.withTx(tx => adminVerdict(tx, ctx.sender));
    denyIfNotAdmin(verdict);
    return sendEmail(ctx, args);
  }
);

export const cancel_email = spacetimedb.procedure(
  { resendId: t.string() },
  t.unit(),
  (ctx, args) => {
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
    ensureOkOrThrow(response, 'resend.cancel_email_failed');

    ctx.withTx(tx => {
      const existing = tx.db.resendEmail.resendId.find(args.resendId);
      if (!existing) return;
      const updated = {
        ...existing,
        status: EmailStatus.Cancelled,
        updatedAt: ctx.timestamp,
      };
      if (tx.db.resendEmail.resendId.update) {
        tx.db.resendEmail.resendId.update(updated);
      } else {
        tx.db.resendEmail.delete(existing);
        tx.db.resendEmail.insert(updated);
      }
    });
    return {};
  }
);

export const get_email = spacetimedb.procedure(
  { resendId: t.string() },
  t.option(resendEmailTable.rowType),
  (ctx, { resendId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(
      tx => tx.db.resendEmail.resendId.find(resendId) ?? undefined
    );
  }
);

export const list_emails_by_user_id = spacetimedb.procedure(
  { userId: t.string() },
  t.array(resendEmailTable.rowType),
  (ctx, { userId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx =>
      takeRows(tx.db.resendEmail.byUserId.filter(userId))
    );
  }
);

export const list_emails_by_org_id = spacetimedb.procedure(
  { orgId: t.string() },
  t.array(resendEmailTable.rowType),
  (ctx, { orgId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx => takeRows(tx.db.resendEmail.byOrgId.filter(orgId)));
  }
);

export const list_emails_by_status = spacetimedb.procedure(
  { status: emailStatus },
  t.array(resendEmailTable.rowType),
  (ctx, { status }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx =>
      takeRows(tx.db.resendEmail.byStatus.filter(status))
    );
  }
);

export const list_delivery_events_for_email = spacetimedb.procedure(
  { resendId: t.string() },
  t.array(resendDeliveryEventTable.rowType),
  (ctx, { resendId }) => {
    requireProcedureAdmin(ctx);
    return ctx.withTx(tx =>
      takeRows(tx.db.resendDeliveryEvent.byResendId.filter(resendId))
    );
  }
);

export const resend_api_request = spacetimedb.procedure(
  {
    method: t.string(),
    path: t.string(),
    jsonBody: t.option(t.string()),
    idempotencyKey: t.option(t.string()),
  },
  t.object('ResendApiRequestResult', {
    status: t.u16(),
    body: t.string(),
  }),
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
