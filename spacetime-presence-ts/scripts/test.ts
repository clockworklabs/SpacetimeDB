import { SenderError, Timestamp } from 'spacetimedb';
import {
  buildPresenceKey,
  installPresenceConfig,
  MAX_PRESENCE_SWEEP_BATCH,
  removePresence,
  runPresenceSweep,
  touchPresence,
  updatePresenceConfig,
  upsertPresence,
  type PresenceEntryRow,
} from '../src/presence.ts';

let pass = 0;
let fail = 0;

function assert(cond: boolean, name: string, detail = ''): void {
  if (cond) {
    pass++;
    process.stdout.write(`  ok   ${name}\n`);
  } else {
    fail++;
    process.stdout.write(
      `  FAIL ${name}${detail ? `\n       ${detail}` : ''}\n`
    );
  }
}

function assertThrows(fn: () => void, expected: string, name: string): void {
  try {
    fn();
    assert(false, name, `expected ${expected}`);
  } catch (error) {
    assert(error instanceof SenderError && error.message === expected, name);
  }
}

type ConfigRow = {
  singleton: boolean;
  defaultTtlSeconds: number;
  sweepBatch: number;
  updatedAt: Timestamp;
};

function makeTx(nowMicros = 0n, config?: Partial<ConfigRow>) {
  const rows = new Map<string, PresenceEntryRow>();
  const configRow: ConfigRow = {
    singleton: true,
    defaultTtlSeconds: 30,
    sweepBatch: 500,
    updatedAt: new Timestamp(0n),
    ...config,
  };
  const tx = {
    timestamp: new Timestamp(nowMicros),
    db: {
      presenceConfig: {
        singleton: { find: () => configRow },
      },
      presenceEntry: {
        key: {
          find: (key: string) => rows.get(key),
          update: (row: PresenceEntryRow) => rows.set(row.key, row),
        },
        insert: (row: PresenceEntryRow) => rows.set(row.key, row),
        delete: (row: PresenceEntryRow) => rows.delete(row.key),
      },
    },
    rows,
  };
  return tx;
}

process.stdout.write('presence helpers\n');

{
  const tx = makeTx(0n, { sweepBatch: 2 });
  for (const subject of ['fresh-a', 'expired-a', 'fresh-b', 'expired-b']) {
    upsertPresence(tx, {
      scope: 'room:1',
      subject,
      ttlSeconds: subject.startsWith('fresh') ? 100 : 1,
    });
  }
  upsertPresence(tx, { scope: 'room:1', subject: 'expired-c', ttlSeconds: 1 });
  tx.timestamp = new Timestamp(2_000_000n);
  const deleted = runPresenceSweep(tx, [...tx.rows.values()]);
  assert(deleted === 2, 'sweep skips unexpired rows and stops at the batch');
  assert(
    tx.rows.has(buildPresenceKey('room:1', 'fresh-a')) &&
      tx.rows.has(buildPresenceKey('room:1', 'fresh-b')) &&
      tx.rows.size === 3,
    'sweep keeps unexpired rows'
  );
}

{
  const tx = makeTx(0n, { defaultTtlSeconds: 45 });
  const row = upsertPresence(tx, { scope: 'room:1', subject: 'user:alice' });
  assert(
    row.expiresAt.microsSinceUnixEpoch === 45_000_000n,
    'upsert uses the configured default TTL'
  );
  tx.timestamp = new Timestamp(10_000_000n);
  const touched = touchPresence(tx, 'room:1', 'user:alice');
  assert(
    touched.expiresAt.microsSinceUnixEpoch === 55_000_000n,
    'touch uses the configured default TTL'
  );
}

{
  const tx = makeTx();
  assertThrows(
    () => upsertPresence(tx, { scope: ' ', subject: 'user:alice' }),
    'presence.invalid_scope',
    'upsert rejects an empty scope with a SenderError'
  );
  assertThrows(
    () =>
      upsertPresence(tx, {
        scope: 'room:1',
        subject: 'user:alice',
        payloadJson: 'x'.repeat(4097),
      }),
    'presence.invalid_payload',
    'upsert rejects an oversized payload'
  );
}

assert(
  buildPresenceKey('room::one', 'user') !==
    buildPresenceKey('room', 'one::user'),
  'compound keys cannot collide through delimiters'
);

{
  const tx = makeTx();
  upsertPresence(tx, {
    scope: 'room:1',
    subject: 'user:alice',
    status: 'away',
    activity: 'editing',
    payloadJson: '{"cursor":4}',
    ttlSeconds: 30,
  });
  tx.timestamp = new Timestamp(10_000_000n);
  const row = touchPresence(tx, 'room:1', 'user:alice', 30);
  assert(
    row.status === 'away' &&
      row.activity === 'editing' &&
      row.payloadJson === '{"cursor":4}',
    'touch preserves presence metadata'
  );
}

{
  const tx = makeTx();
  upsertPresence(tx, {
    scope: 'room:1',
    subject: 'user:alice',
    status: 'online',
    ttlSeconds: 30,
  });
  tx.timestamp = new Timestamp(10_000_000n);
  const row = upsertPresence(tx, {
    scope: 'room:1',
    subject: 'user:alice',
    status: 'away',
    ttlSeconds: 30,
  });
  assert(row.status === 'away', 'upsert updates status');
  assert(tx.rows.size === 1, 'upsert keeps one row');
}

{
  const tx = makeTx();
  upsertPresence(tx, {
    scope: 'room:1',
    subject: 'user:alice',
    ttlSeconds: 10,
  });
  const removed = removePresence(tx, 'room:1', 'user:alice');
  assert(removed, 'removePresence returns true for existing row');
  assert(tx.rows.size === 0, 'removePresence deletes row');
}

process.stdout.write('\npresence config\n');

{
  let config: ConfigRow | undefined;
  const ctx = {
    timestamp: new Timestamp(1n),
    db: {
      presenceConfig: {
        singleton: {
          find: () => config,
          update: (row: ConfigRow) => {
            config = row;
          },
        },
        insert: (row: ConfigRow) => {
          config = row;
        },
      },
    },
  };
  installPresenceConfig(ctx, { defaultTtlSeconds: 30, sweepBatch: 500 });
  installPresenceConfig(ctx, { defaultTtlSeconds: 60, sweepBatch: 100 });
  assert(config?.defaultTtlSeconds === 30, 'install keeps the stored config');
  ctx.timestamp = new Timestamp(2n);
  updatePresenceConfig(ctx, { defaultTtlSeconds: 45, sweepBatch: 750 });
  assert(
    config?.defaultTtlSeconds === 45 && config.sweepBatch === 750,
    'configuration updates an existing row'
  );
  assertThrows(
    () =>
      updatePresenceConfig(ctx, {
        defaultTtlSeconds: 45,
        sweepBatch: MAX_PRESENCE_SWEEP_BATCH + 1,
      }),
    'presence.invalid_sweep_batch',
    'configuration rejects an excessive batch size'
  );
}

process.stdout.write(`\n${pass} passed, ${fail} failed\n`);
process.exit(fail === 0 ? 0 : 1);
