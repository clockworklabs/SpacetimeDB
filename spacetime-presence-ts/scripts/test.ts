import { Timestamp } from 'spacetimedb';
import {
  buildPresenceKey,
  installPresenceConfig,
  MAX_PRESENCE_SWEEP_BATCH,
  removePresence,
  resolvePresenceSweepBatch,
  sweepPresence,
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
    assert(error instanceof Error && error.message === expected, name);
  }
}

function makeTx(nowMicros = 0n) {
  const rows = new Map<string, PresenceEntryRow>();
  const tx = {
    timestamp: new Timestamp(nowMicros),
    db: {
      presenceEntry: {
        key: {
          find: (key: string) => rows.get(key),
          update: (row: PresenceEntryRow) => rows.set(row.key, row),
        },
        insert: (row: PresenceEntryRow) => rows.set(row.key, row),
        delete: (row: PresenceEntryRow) => rows.delete(row.key),
        expiresAt: {
          filter: function* () {
            yield* [...rows.values()].sort((a, b) =>
              a.expiresAt.microsSinceUnixEpoch <
              b.expiresAt.microsSinceUnixEpoch
                ? -1
                : 1
            );
          },
        },
      },
    },
    rows,
  };
  return tx;
}

{
  const tx = makeTx();
  upsertPresence(tx, {
    scope: 'room:1',
    subject: 'fresh-a',
    ttlSeconds: 100,
  });
  upsertPresence(tx, {
    scope: 'room:1',
    subject: 'fresh-b',
    ttlSeconds: 100,
  });
  upsertPresence(tx, {
    scope: 'room:1',
    subject: 'expired',
    ttlSeconds: 1,
  });
  tx.timestamp = new Timestamp(2_000_000n);
  const deleted = sweepPresence(tx, tx.db.presenceEntry.expiresAt.filter(), 2);
  assert(deleted === 1, 'sweep reaches expired rows beyond fresh inserts');
  assert(
    ![...tx.rows.values()].some(row => row.subject === 'expired'),
    'indexed sweep removes the expired row'
  );
}

process.stdout.write('\npresence submodule\n');

{
  let config:
    | {
        singleton: boolean;
        defaultTtlSeconds: number;
        sweepBatch: number;
        updatedAt: Timestamp;
      }
    | undefined;
  const ctx = {
    timestamp: new Timestamp(1n),
    db: {
      presenceConfig: {
        singleton: {
          find: () => config,
          update: (row: typeof config) => {
            config = row;
          },
        },
        insert: (row: NonNullable<typeof config>) => {
          config = row;
        },
      },
    },
  };
  installPresenceConfig(ctx, { defaultTtlSeconds: 30, sweepBatch: 500 });
  ctx.timestamp = new Timestamp(2n);
  updatePresenceConfig(ctx, { defaultTtlSeconds: 45, sweepBatch: 750 });
  assert(
    config?.defaultTtlSeconds === 45 && config.sweepBatch === 750,
    'configuration updates an existing row'
  );
  assert(
    resolvePresenceSweepBatch(ctx) === 750,
    'sweep reads the updated batch size'
  );
}

{
  const tx = makeTx();
  assertThrows(
    () => sweepPresence(tx, [], MAX_PRESENCE_SWEEP_BATCH + 1),
    'presence.invalid_sweep_batch',
    'sweep rejects an excessive batch size'
  );
}

assert(
  buildPresenceKey('room::one', 'user') !==
    buildPresenceKey('room', 'one::user'),
  'compound keys cannot collide through delimiters'
);

{
  const tx = makeTx();
  const row = upsertPresence(tx, {
    scope: 'room:1',
    subject: 'user:alice',
    status: 'online',
    ttlSeconds: 30,
  });
  assert(row.scope === 'room:1', 'inserts row');
  assert(tx.rows.size === 1, 'row count 1 after insert');
}

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
    ttlSeconds: 1,
  });
  tx.timestamp = new Timestamp(2_000_000n);
  const deleted = sweepPresence(tx, tx.db.presenceEntry.expiresAt.filter());
  assert(deleted === 1, 'sweep removes expired row');
  assert(tx.rows.size === 0, 'rows empty after sweep');
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

process.stdout.write(`\n${pass} passed, ${fail} failed\n`);
process.exit(fail === 0 ? 0 : 1);
