import * as assert from 'node:assert/strict';
import { Timestamp } from 'spacetimedb';
import { SenderError } from 'spacetimedb/server';
import {
  enqueueEmailInTx,
  finishQueuedEmail,
  sendQueuedEmail,
} from '../src/submodule/queue';

// The mock context implements only the tables these paths read and write.
type Row = { id: bigint; name?: string };
function table() {
  const rows = new Map<bigint, Row & Record<string, unknown>>();
  let next = 0n;
  return {
    rows,
    insert(row: Row) {
      const inserted = { ...row, id: ++next };
      rows.set(inserted.id, inserted);
      return inserted;
    },
  };
}
type Email = { resendId: string; status: { tag: string } };
const emails = new Map<string, Email>();
let configured = true;
const outbox = table();
const jobs = table();
const fires = table();
const tx = {
  timestamp: new Timestamp(1_000_000n),
  newUuidV7: () => '0198-uuid',
  db: {
    resendOutbox: {
      ...outbox,
      id: {
        find: (id: bigint) => outbox.rows.get(id) ?? null,
        update: (row: Row) => outbox.rows.set(row.id, row),
      },
      delete: (row: Row) => outbox.rows.delete(row.id),
    },
    resendConfig: {
      singleton: {
        find: () =>
          configured ? { apiKey: 're_test', defaultFrom: 'a@b.dev' } : null,
      },
    },
    resendEmail: {
      resendId: {
        find: (id: string) => emails.get(id) ?? null,
        update: (row: Email) => emails.set(row.resendId, row),
      },
      insert: (row: Email) => emails.set(row.resendId, row),
      delete: (row: Email) => emails.delete(row.resendId),
    },
    retryJob: {
      ...jobs,
      name: {
        filter: (name: string) =>
          [...jobs.rows.values()].filter(r => r.name === name).values(),
      },
    },
    retryJobFire: { insert: (row: Row) => fires.insert(row) },
  },
};
const enqueue = (args: Parameters<typeof enqueueEmailInTx>[1]) =>
  enqueueEmailInTx(tx as never, args);

const id = enqueue({
  to: ['user@example.com'],
  subject: 'Welcome',
  text: 'Hello',
  idempotencyKey: 'welcome-1',
});
const row = outbox.rows.get(id)!;
assert.equal(row.idempotencyKey, 'welcome-1');
const job = jobs.rows.get(row.jobId as bigint)!;
assert.equal(job.name, `email:${id}`);
assert.equal(job.queue, 'email');
assert.deepEqual(job.args, { tag: 'sendEmail', value: { outboxId: id } });
assert.equal(fires.rows.size, 2);

assert.throws(() => enqueue({ to: [], subject: 'x', text: 'y' }), {
  message: 'resend.send_email_no_recipients',
});
assert.equal(outbox.rows.size, 1);

// The send handler.
let response = { status: 200, body: '{"id":"re_1"}' };
const sent: { key: string | undefined }[] = [];
const proc = {
  timestamp: tx.timestamp,
  withTx: <T>(body: (ctx: typeof tx) => T) => body(tx),
  http: {
    fetch(_url: string, init: { headers: Record<string, string> }) {
      sent.push({ key: init.headers['Idempotency-Key'] });
      return { status: response.status, text: () => response.body };
    },
  },
};
const jobInfo = {
  id: 1n,
  name: 'email:1',
  attempt: 1,
  idempotencyKey: 'retry-k',
};
const attempt = () => sendQueuedEmail(proc as never, { outboxId: id }, jobInfo);
const outcome = (status: 'Succeeded' | 'Failed' | 'Cancelled') => ({
  id: 1n,
  name: `email:${id}`,
  queue: 'email',
  args: { tag: 'sendEmail' as const, value: { outboxId: id } },
  status,
  attempts: 1,
  error: status === 'Failed' ? 'x' : undefined,
});

configured = false;
assert.deepEqual(attempt(), { ok: false, error: 'resend.config_not_set' });
configured = true;

// The caller's own key wins over the job's.
assert.deepEqual(attempt(), { ok: true });
assert.equal(sent.at(-1)?.key, 'welcome-1');
assert.equal(emails.get('re_1')?.status.tag, 'Queued');
assert.equal(outbox.rows.get(id)?.resendId, 're_1');

// 401, 403, 408, 429, 5xx and a concurrent 409 are retried.
for (const [status, body] of [
  [401, '{}'],
  [403, '{}'],
  [408, '{}'],
  [429, '{}'],
  [500, '{}'],
  [409, '{"name":"concurrent_idempotent_requests"}'],
] as const) {
  response = { status, body };
  assert.equal(attempt().ok, false, `status ${status}`);
}
// Other 4xx and a reused key with a different email are permanent.
for (const [status, body] of [
  [422, '{"name":"validation_error"}'],
  [409, '{"name":"invalid_idempotent_request"}'],
] as const) {
  response = { status, body };
  assert.throws(attempt, SenderError, `status ${status}`);
}

// A job that finishes without Resend accepting the email leaves a row.
outbox.rows.set(id, { ...outbox.rows.get(id)!, resendId: undefined });
finishQueuedEmail(tx as never, outcome('Cancelled'));
assert.equal(emails.get(`outbox:${id}`)?.status.tag, 'Cancelled');
assert.equal(outbox.rows.has(id), false);

// If Resend accepts it afterwards, the real row replaces that one.
response = { status: 200, body: '{"id":"re_2"}' };
outbox.rows.set(id, { id, jobId: 1n, to: ['user@example.com'], subject: 'W' });
assert.deepEqual(attempt(), { ok: true });
assert.equal(emails.has(`outbox:${id}`), false);
assert.equal(emails.has('re_2'), true);

// A failed job whose email Resend did accept doesn't get a failed row.
finishQueuedEmail(tx as never, outcome('Failed'));
assert.equal(emails.has(`outbox:${id}`), false);
assert.equal(outbox.rows.has(id), false);

process.stdout.write('resend queue tests passed\n');
