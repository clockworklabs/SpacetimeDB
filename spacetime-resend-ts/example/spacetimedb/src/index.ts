// The browser never talks to Resend directly and is never granted admin. It calls
// host procedures that forward to the submodule through `ctx.as.resend`. Resend's
// base tables stay private; caller-scoped views below expose only the current
// connection's dispatches.

import { schema, table, t, SenderError, Router } from 'spacetimedb/server';
import * as resend from '@spacetimedb/resend/submodule';
import * as rateLimit from '@spacetimedb/rate-limit/submodule';
import { messageHtml } from './message';

const dispatchPolicy = table(
  { name: 'dispatch_policy', public: false },
  {
    singleton: t.bool().primaryKey(),
    allowedRecipientsJson: t.string(),
    updatedAt: t.timestamp(),
  }
);

const spacetimedb = schema({
  resend,
  rateLimit,
  dispatchPolicy,
});

function subjectFor(ctx: { sender: { toHexString(): string } }): string {
  return ctx.sender.toHexString();
}

export const myDispatchEmails = spacetimedb.view(
  { name: 'my_dispatch_emails', public: true },
  resend.t.array(resend.resendEmailTable.rowType),
  ctx => [...ctx.db.resend.resendEmail.byUserId.filter(subjectFor(ctx))]
);

export const myDispatchDeliveryEvents = spacetimedb.view(
  { name: 'my_dispatch_delivery_events', public: true },
  resend.t.array(resend.resendDeliveryEventTable.rowType),
  ctx => {
    const out = [];
    for (const email of ctx.db.resend.resendEmail.byUserId.filter(
      subjectFor(ctx)
    )) {
      for (const event of ctx.db.resend.resendDeliveryEvent.byResendId.filter(
        email.resendId
      )) {
        out.push(event);
      }
    }
    return out;
  }
);

const MAX_SUBJECT = 200;
const MAX_MESSAGE = 5000;
const MAX_ALLOWED_RECIPIENTS = 10;
const MAX_RECIPIENT_POLICY_LENGTH = 4096;
const CALLER_SEND_LIMIT = 5;
const CALLER_WINDOW_SECONDS = 10 * 60;
const GLOBAL_SEND_LIMIT = 25;
const GLOBAL_WINDOW_SECONDS = 60 * 60;

function fail(message: string): never {
  throw new SenderError(`dispatch.${message}`);
}

function normalizeEmail(email: string): string {
  const out = email.trim().toLowerCase();
  if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(out)) fail('invalid_email');
  return out;
}

function parseAllowedRecipients(value: string): string[] {
  if (value.length > MAX_RECIPIENT_POLICY_LENGTH) {
    fail('recipient_policy_too_large');
  }
  let parsed: unknown;
  try {
    parsed = JSON.parse(value);
  } catch {
    fail('invalid_recipient_policy');
  }
  if (!Array.isArray(parsed) || parsed.length === 0) {
    fail('invalid_recipient_policy');
  }
  const recipients = [
    ...new Set(parsed.map(value => normalizeEmail(String(value)))),
  ];
  if (recipients.length > MAX_ALLOWED_RECIPIENTS) {
    fail('too_many_allowed_recipients');
  }
  return recipients;
}

function clean(
  value: string | undefined,
  fallback: string,
  max: number
): string {
  const out = (value ?? '').trim().replace(/\s+/g, ' ');
  return (out || fallback).slice(0, max);
}

const dispatchSendResult = t.object('DispatchSendResult', {
  ok: t.bool(),
  resendId: t.option(t.string()),
  message: t.string(),
});

export const set_dispatch_policy = spacetimedb.procedure(
  { allowedRecipientsJson: t.string() },
  t.bool(),
  (ctx, args) => {
    const isAdmin = ctx.withTx(
      tx => tx.db.resend.resendAdminIdentity.identity.find(ctx.sender) != null
    );
    if (!isAdmin) fail('not_authorized');
    const recipients = parseAllowedRecipients(args.allowedRecipientsJson);
    ctx.withTx(tx => {
      const existing = tx.db.dispatchPolicy.singleton.find(true);
      const row = {
        singleton: true,
        allowedRecipientsJson: JSON.stringify(recipients),
        updatedAt: ctx.timestamp,
      };
      if (existing) tx.db.dispatchPolicy.singleton.update(row);
      else tx.db.dispatchPolicy.insert(row);
    });
    return true;
  }
);

// The host policy restricts recipients and applies caller and global quotas before
// delegating delivery through the private Resend configuration.
export const send_dispatch = spacetimedb.procedure(
  {
    to: t.string(),
    subject: t.string(),
    message: t.string(),
  },
  dispatchSendResult,
  (ctx, args) => {
    const to = normalizeEmail(args.to);
    const subject = clean(args.subject, '(no subject)', MAX_SUBJECT);
    const message = (args.message ?? '').trim().slice(0, MAX_MESSAGE);
    if (!message) fail('empty_message');

    const policyJson = ctx.withTx(
      tx =>
        tx.db.dispatchPolicy.singleton.find(true)?.allowedRecipientsJson ?? null
    );
    if (policyJson == null) fail('policy_missing');
    const allowed = parseAllowedRecipients(policyJson);
    if (!allowed.includes(to)) fail('recipient_not_allowed');

    const authorization = ctx.withTx(tx => {
      const caller = rateLimit.consumeRateLimit(tx.as.rateLimit, {
        key: `dispatch:caller:${subjectFor(ctx)}`,
        scope: 'dispatch.send.caller',
        limit: CALLER_SEND_LIMIT,
        windowSeconds: CALLER_WINDOW_SECONDS,
      });
      if (!caller.allowed) return 'rate_limited' as const;
      const global = rateLimit.consumeRateLimit(tx.as.rateLimit, {
        key: 'dispatch:global',
        scope: 'dispatch.send.global',
        limit: GLOBAL_SEND_LIMIT,
        windowSeconds: GLOBAL_WINDOW_SECONDS,
      });
      return global.allowed ? ('allowed' as const) : ('rate_limited' as const);
    });
    if (authorization !== 'allowed') fail(authorization);

    try {
      const result = resend.sendEmail(ctx.as.resend, {
        to: [to],
        subject,
        html: messageHtml(message),
        text: message,
        tagsJson: JSON.stringify([
          { name: 'source', value: 'dispatch' },
          { name: 'userId', value: subjectFor(ctx) },
        ]),
      });
      return {
        ok: true,
        resendId: result.resendId,
        message: `Dispatched to ${to}.`,
      };
    } catch {
      return {
        ok: false,
        resendId: undefined,
        message: 'dispatch.delivery_failed',
      };
    }
  }
);

const dispatchDeleteResult = t.object('DispatchDeleteResult', {
  ok: t.bool(),
  removed: t.u32(),
});

// Remove a single dispatch and any delivery events it collected. This is a demo
// convenience so the log can be pruned; it writes directly to the submodule tables.
export const delete_dispatch = spacetimedb.procedure(
  { resendId: t.string() },
  dispatchDeleteResult,
  (ctx, args) => {
    return ctx.withTx(tx => {
      let removed = 0;
      const row = tx.db.resend.resendEmail.resendId.find(args.resendId);
      if (row && row.userId === subjectFor(ctx)) {
        tx.db.resend.resendEmail.delete(row);
        removed += 1;
        for (const event of tx.db.resend.resendDeliveryEvent.byResendId.filter(
          args.resendId
        )) {
          tx.db.resend.resendDeliveryEvent.delete(event);
        }
      }
      return { ok: true, removed };
    });
  }
);

export const clear_dispatches = spacetimedb.procedure(
  {},
  dispatchDeleteResult,
  ctx => {
    return ctx.withTx(tx => {
      let removed = 0;
      const owned = [
        ...tx.db.resend.resendEmail.byUserId.filter(subjectFor(ctx)),
      ];
      for (const row of owned) {
        for (const event of tx.db.resend.resendDeliveryEvent.byResendId.filter(
          row.resendId
        )) {
          tx.db.resend.resendDeliveryEvent.delete(event);
        }
        tx.db.resend.resendEmail.delete(row);
        removed += 1;
      }
      return { ok: true, removed };
    });
  }
);

// Resend posts delivery webhooks straight to the database over a native STDB HTTP
// route. The submodule verifies the svix signature in-module (via crypto-ts) and
// ingests. No Node relay does any of this work.
const resendWebhookHandler = resend.makeResendWebhookHandler();
export const resend_webhook = spacetimedb.httpHandler((ctx, req) =>
  resendWebhookHandler(ctx.as.resend, req)
);
export const router = spacetimedb.httpRouter(
  new Router().post('/webhook/resend', resend_webhook)
);

export const init = spacetimedb.init(ctx => {
  resend.installResend(ctx.as.resend);
  rateLimit.installRateLimit(ctx.as.rateLimit);
});

export default spacetimedb;
