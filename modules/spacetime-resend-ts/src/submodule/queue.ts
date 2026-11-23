import * as v from 'valibot';
import { SenderError } from 'spacetimedb/server';
import {
  retryFailed,
  retryOk,
  type RetryJobInfo,
  type RetryOutcome,
  type RetryProcedureExport,
  type RetryResult,
} from '@spacetimedb/retry';
import {
  EmailStatus,
  emailQueue,
  spacetimedb,
  vResendErrorBody,
  vSendEmailResponse,
  type ProcedureModuleCtx,
  type WriteCtx,
} from './schema.js';
import { callResend, resendErrorSuffix } from './http.js';
import {
  buildSendEmailBody,
  extractTagFieldsFromJson,
  recordQueuedEmail,
  type SendEmailArgs,
} from './operations.js';
import { upsertEmail } from './email_writes.js';
import { validateEmailInput } from './email-input.js';
import { safeJsonParse, throwSenderError } from './validation.js';
import { errors } from './errors.js';

const MAX_ATTEMPTS = 8;
const BACKOFF_SECONDS = 5;

/**
 * A rejected request that will be rejected again. 401 and 403 are retried so a
 * rotated API key can be fixed while emails wait. A 409 is permanent only when
 * the idempotency key was already used for a different email.
 */
function isPermanent(status: number, body: string): boolean {
  if (status === 409) {
    const parsed = v.safeParse(vResendErrorBody, safeJsonParse(body));
    return (
      parsed.success && parsed.output.name === 'invalid_idempotent_request'
    );
  }
  return (
    status >= 400 && status < 500 && ![401, 403, 408, 429].includes(status)
  );
}

/**
 * Queue an email in the caller's transaction and return its outbox ID. It is
 * sent only if that transaction commits, with up to 8 attempts over about 10
 * minutes. Throws a `SenderError` for invalid input. The caller authorizes.
 */
export function enqueueEmailInTx(ctx: WriteCtx, args: SendEmailArgs): bigint {
  try {
    validateEmailInput(args);
  } catch (error) {
    throwSenderError(
      error instanceof Error ? error.message : errors.sendEmailInvalidInput
    );
  }
  const row = ctx.db.resendOutbox.insert({
    id: 0n,
    jobId: 0n,
    from: args.from,
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
    idempotencyKey: args.idempotencyKey,
    resendId: undefined,
    createdAt: ctx.timestamp,
  });
  const jobId = emailQueue.enqueue(ctx, {
    name: `email:${row.id}`,
    queue: 'email',
    args: { tag: 'sendEmail', value: { outboxId: row.id } },
    maxAttempts: MAX_ATTEMPTS,
    backoffSecs: BACKOFF_SECONDS,
  });
  ctx.db.resendOutbox.id.update({ ...row, jobId });
  return row.id;
}

/**
 * Cancel a queued email. Returns false if it was already sent, failed or
 * cancelled. An attempt already in flight may still send it. The caller
 * authorizes.
 */
export function cancelQueuedEmail(ctx: WriteCtx, outboxId: bigint): boolean {
  const row = ctx.db.resendOutbox.id.find(outboxId);
  return row ? emailQueue.cancel(ctx, row.jobId) : false;
}

/**
 * Limit how many queued emails are sent at once. 0 pauses sending. Without a
 * limit, up to 1,000 run at once and bursts can hit Resend's rate limit, which
 * is retried. The caller authorizes.
 */
export function setEmailQueueLimit(
  ctx: WriteCtx,
  maxParallelism: number
): void {
  emailQueue.setQueue(ctx, 'email', { maxParallelism });
}

/** One send attempt for a queued email. */
export function sendQueuedEmail(
  ctx: ProcedureModuleCtx,
  { outboxId }: { outboxId: bigint },
  job: RetryJobInfo
): RetryResult {
  const { row, cfg } = ctx.withTx(tx => ({
    row: tx.db.resendOutbox.id.find(outboxId),
    cfg: tx.db.resendConfig.singleton.find(true),
  }));
  if (!row) throw new SenderError(errors.queuedEmailMissing);
  if (!cfg) return retryFailed(errors.configNotSet);
  const from = row.from ?? cfg.defaultFrom;
  if (!from) throw new SenderError(errors.sendEmailMissingFrom);

  const response = callResend(ctx, {
    method: 'POST',
    path: '/emails',
    apiKey: cfg.apiKey,
    jsonBody: buildSendEmailBody({ ...row, from }),
    idempotencyKey: row.idempotencyKey ?? job.idempotencyKey,
  });
  if (response.status < 200 || response.status >= 300) {
    const error = `${errors.sendEmailFailed}:${response.status}${resendErrorSuffix(response.body)}`;
    if (isPermanent(response.status, response.body)) {
      throw new SenderError(error);
    }
    return retryFailed(error);
  }
  const parsed = v.safeParse(vSendEmailResponse, safeJsonParse(response.body));
  if (!parsed.success) return retryFailed(errors.sendEmailInvalidResponse);
  const resendId = parsed.output.id;
  // Resend has the email even if the job was cancelled or timed out meanwhile,
  // so this replaces any `outbox:<id>` row the job left.
  ctx.withTx(tx => {
    recordQueuedEmail(tx, ctx.timestamp, {
      resendId,
      from,
      to: row.to,
      subject: row.subject,
      html: row.html,
      text: row.text,
      tagsJson: row.tagsJson,
    });
    const current = tx.db.resendOutbox.id.find(outboxId);
    if (current) tx.db.resendOutbox.id.update({ ...current, resendId });
    const placeholder = tx.db.resendEmail.resendId.find(`outbox:${outboxId}`);
    if (placeholder) tx.db.resendEmail.delete(placeholder);
  });
  return retryOk();
}

/**
 * Delete the outbox row when the job finishes. An email Resend never accepted
 * is kept as a `resend_email` row with ID `outbox:<id>`, so listings by status
 * include it.
 */
export function finishQueuedEmail(
  tx: WriteCtx,
  outcome: RetryOutcome<{ tag: 'sendEmail'; value: { outboxId: bigint } }>
): void {
  const outboxId = outcome.args.value.outboxId;
  const row = tx.db.resendOutbox.id.find(outboxId);
  if (!row) return;
  tx.db.resendOutbox.delete(row);
  if (outcome.status === 'Succeeded' || row.resendId !== undefined) return;
  upsertEmail(tx, tx.timestamp, {
    resendId: `outbox:${outboxId}`,
    fromAddress: row.from ?? '',
    toAddressesJson: JSON.stringify(row.to),
    subject: row.subject,
    html: row.html,
    text: row.text,
    status:
      outcome.status === 'Failed' ? EmailStatus.Failed : EmailStatus.Cancelled,
    lastError: outcome.error,
    bouncedAt: undefined,
    bounceJson: undefined,
    failedAt: outcome.status === 'Failed' ? tx.timestamp : undefined,
    failureReason: outcome.error,
    complained: false,
    complainedAt: undefined,
    opened: false,
    openedAt: undefined,
    clicked: false,
    clickedAt: undefined,
    deliveredAt: undefined,
    sentAt: undefined,
    tagsJson: row.tagsJson,
    ...extractTagFieldsFromJson(row.tagsJson),
  });
}

/** Sends queued emails. Export it from the module so the host registers it. */
export const resendSendEmailJob: RetryProcedureExport<ProcedureModuleCtx> =
  emailQueue.retryProcedure(
    spacetimedb,
    { sendEmail: sendQueuedEmail },
    { onComplete: finishQueuedEmail }
  );
