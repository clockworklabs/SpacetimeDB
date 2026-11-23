import * as assert from 'node:assert/strict';
import { Timestamp } from 'spacetimedb';
import { enqueueEmailInTx } from '../src/submodule/queue';

// The mock context implements only the tables `enqueueEmailInTx` writes.
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
const outbox = table();
const jobs = table();
const fires = table();
const tx = {
  timestamp: new Timestamp(1_000_000n),
  db: {
    resendOutbox: {
      ...outbox,
      id: { update: (row: Row) => outbox.rows.set(row.id, row) },
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

process.stdout.write('resend queue tests passed\n');
