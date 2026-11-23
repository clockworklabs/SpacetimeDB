import * as v from 'valibot';
import { SenderError } from 'spacetimedb/server';
import {
  retryFailed,
  retryOk,
  type RetryProcedureExport,
} from '@spacetimedb/retry';
import {
  EmailStatus,
  emailQueue,
  spacetimedb,
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

/** 4xx responses other than 408, 409 and 429 will not succeed on retry. */
function isPermanent(status: number): boolean {
  return (
    status >= 400 &&
    status < 500 &&
    status !== 408 &&
    status !== 409 &&
    status !== 429
  );
}

/**
 * Queue an email in the caller's transaction and return its outbox ID. It is
 * sent only if that transaction commits, and retried until Resend accepts it.
 * Throws a `SenderError` for invalid input. The caller authorizes.
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
    createdAt: ctx.timestamp,
  });
  const jobId = emailQueue.enqueue(ctx, {
    name: `email:${row.id}`,
    queue: 'email',
    args: { tag: 'sendEmail', value: { outboxId: row.id } },
    maxAttempts: 8,
    backoffSecs: 5,
  });
  ctx.db.resendOutbox.id.update({ ...row, jobId });
  return row.id;
}

/** Sends queued emails. Export it from the module so the host registers it. */
export const resendSendEmailJob: RetryProcedureExport<ProcedureModuleCtx> =
  emailQueue.retryProcedure(
    spacetimedb,
    {
      sendEmail(ctx, { outboxId }, job) {
        const loaded = ctx.withTx(tx => ({
          row: tx.db.resendOutbox.id.find(outboxId),
          cfg: tx.db.resendConfig.singleton.find(true),
        }));
        const { row, cfg } = loaded;
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
          if (isPermanent(response.status)) throw new SenderError(error);
          return retryFailed(error);
        }
        const parsed = v.safeParse(
          vSendEmailResponse,
          safeJsonParse(response.body)
        );
        if (!parsed.success)
          return retryFailed(errors.sendEmailInvalidResponse);
        ctx.withTx(tx =>
          recordQueuedEmail(tx, ctx.timestamp, {
            resendId: parsed.output.id,
            from,
            to: row.to,
            subject: row.subject,
            html: row.html,
            text: row.text,
            tagsJson: row.tagsJson,
          })
        );
        return retryOk();
      },
    },
    {
      // A queued email that never reached Resend is kept as a `resend_email` row
      // with ID `outbox:<id>`, so listings by status include it.
      onComplete(tx, outcome) {
        const outboxId = outcome.args.value.outboxId;
        const row = tx.db.resendOutbox.id.find(outboxId);
        if (!row) return;
        tx.db.resendOutbox.delete(row);
        if (outcome.status === 'Succeeded') return;
        upsertEmail(tx, tx.timestamp, {
          resendId: `outbox:${outboxId}`,
          fromAddress: row.from ?? '',
          toAddressesJson: JSON.stringify(row.to),
          subject: row.subject,
          html: row.html,
          text: row.text,
          status:
            outcome.status === 'Failed'
              ? EmailStatus.Failed
              : EmailStatus.Cancelled,
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
      },
    }
  );
