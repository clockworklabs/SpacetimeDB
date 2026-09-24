import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';
import { getSavedMongoDbCheckoutState } from '../src/stacks/backends/saved-mongodb-checkout.js';
import { getSavedPostgresCheckoutState } from '../src/stacks/backends/saved-postgres-checkout.js';

const READERS = { postgres: getSavedPostgresCheckoutState, mongodb: getSavedMongoDbCheckoutState } as const;

function fixture(backend: keyof typeof READERS = 'postgres') {
  const root = mkdtempSync(join(tmpdir(), `saved-${backend}-reader-`)), app = join(root, 'app');
  mkdirSync(app); writeFileSync(join(app, 'server.js'), 'source');
  const sourceSha256 = hashAppSource(app).sha256;
  const mapping = JSON.stringify(backend === 'postgres'
    ? { sourceSha256, sql: "SELECT :'account', :'item'" } : { sourceSha256, script: 'return {};' });
  const reader = { path: join(root, 'reader.json'), sha256: createHash('sha256').update(mapping).digest('hex') };
  writeFileSync(reader.path, mapping);
  const state = { accountId: 'buyer', itemId: 'keyboard', priceMinor: 8900, cart: [],
    stock: [{ warehouseId: 'east', quantity: 10 }], reservations: [], payments: [], orphanOrderLines: 0,
    orphanAllocations: 0, orders: [{ id: 'old', accountId: 'buyer', totalMinor: 8900,
      refundedMinor: 0, status: 'pending', lines: [{ itemId: 'keyboard', quantity: 1, priceMinor: 8900,
        allocations: [{ warehouseId: 'east', quantity: 1 }] }] }] };
  return { root, sourceSha256, state, args: { app, reader, account: "buyer'\\name", item: 'Keyboard',
    lease: { resources: { container: { id: 'owned-id', name: 'owned-name' }, database: 'owned-db' } } } };
}

test('saved reader uses the exact owned container and one read-only repeatable snapshot with psql variables', t => {
  const f = fixture(); t.after(() => rmSync(f.root, { recursive: true, force: true }));
  let queries = 0;
  const exec: TextCommandExecutor = (file, args, options) => {
    assert.equal(file, 'docker');
    if (args[0] === 'inspect') { assert.equal(args.at(-1), 'owned-name'); return 'owned-id'; }
    queries++;
    assert.deepEqual(args.slice(0, 6), ['exec', '-i', 'owned-id', 'psql', '-X', '-qAt']);
    assert(args.includes('owned-db')); assert(args.includes('ON_ERROR_STOP=1'));
    assert(args.includes(`account=${f.args.account}`)); assert(args.includes('item=Keyboard'));
    const sql = String(options.input);
    assert.match(sql, /^BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;/);
    assert.match(sql, /SELECT :'account', :'item'/); assert.match(sql, /COMMIT;\n$/);
    assert(!sql.includes(f.args.account));
    return JSON.stringify({ accountMatches: 1, itemMatches: 1, state: f.state });
  };
  const result = getSavedPostgresCheckoutState({ ...f.args, exec });
  assert.equal(queries, 1); assert.deepEqual(result.state, f.state);
  assert.deepEqual(result.schemaSha256, { source: f.sourceSha256, reader: f.args.reader.sha256 });
  assert.equal(result.scope, 'orders');
});

test('reader and saved source mismatches fail before querying; changed container ownership prevents reads', t => {
  for (const backend of ['postgres', 'mongodb'] as const) {
    const f = fixture(backend); t.after(() => rmSync(f.root, { recursive: true, force: true }));
    const read = READERS[backend];
    const noExec: TextCommandExecutor = () => { throw new Error('must not execute'); };
    assert.throws(() => read({ ...f.args,
      reader: { ...f.args.reader, sha256: '0'.repeat(64) }, exec: noExec }), /reader hash mismatch/);
    writeFileSync(join(f.args.app, 'server.js'), 'changed');
    assert.throws(() => read({ ...f.args, exec: noExec }), /source hash mismatch/);
    writeFileSync(join(f.args.app, 'server.js'), 'source');
    const wrongOwner: TextCommandExecutor = (_file, args) => {
      assert.equal(args[0], 'inspect'); return 'replacement-id';
    };
    assert.throws(() => read({ ...f.args, exec: wrongOwner }), /changed after lease creation/);
  }
});

test('reader rejects missing or ambiguous entities, omitted accounting and fabricated payment state', t => {
  for (const backend of ['postgres', 'mongodb'] as const) {
    const f = fixture(backend); t.after(() => rmSync(f.root, { recursive: true, force: true }));
    const read = READERS[backend];
    const valid = { accountMatches: 1, itemMatches: 1, state: f.state };
    for (const value of [
      { ...valid, accountMatches: 0 }, { ...valid, itemMatches: 2 },
      { ...valid, state: { ...f.state, orphanAllocations: undefined } },
      { ...valid, state: { ...f.state, orders: [{ ...f.state.orders[0], refundedMinor: undefined }] } },
      { ...valid, state: { ...f.state, payments: [{ id: 'made-up', orderId: 'old', amountMinor: 8900, status: 'paid' }] } },
    ]) {
      const exec: TextCommandExecutor = (_file, args) => args[0] === 'inspect' ? 'owned-id' : JSON.stringify(value);
      assert.throws(() => read({ ...f.args, exec }));
    }
    const reservations = [{ itemId: 'keyboard', warehouseId: 'east', quantity: 1 }];
    const exec: TextCommandExecutor = (_file, args) => args[0] === 'inspect' ? 'owned-id'
      : JSON.stringify({ ...valid, state: { ...f.state, reservations } });
    assert.deepEqual(read({ ...f.args, exec }).state.reservations, reservations);
  }
});
