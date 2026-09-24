import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';
import { getSavedMongoDbCheckoutState } from '../src/stacks/backends/saved-mongodb-checkout.js';
import { attemptDatabaseIdentity } from '../src/stacks/hosted-database-identity.js';
import type { BackendLeaseNetwork } from '../src/runtime/backend-lease.js';

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'saved-mongodb-reader-')), app = join(root, 'app');
  mkdirSync(app); writeFileSync(join(app, 'server.js'), 'source');
  const sourceSha256 = hashAppSource(app).sha256;
  const mapping = JSON.stringify({ sourceSha256, script: "return {};" });
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

test('saved reader uses the owned Mongo container and aborts its consistent snapshot', t => {
  const f = fixture(); t.after(() => rmSync(f.root, { recursive: true, force: true }));
  let queries = 0;
  const exec: TextCommandExecutor = (file, args, options) => {
    assert.equal(file, 'docker');
    if (args[0] === 'inspect') { assert.equal(args.at(-1), 'owned-name'); return 'owned-id'; }
    queries++;
    assert.deepEqual(args.slice(0, 4), ['exec', 'owned-id', 'mongosh', 'owned-db']);
    const script = String(args.at(-1));
    assert(script.includes(`account=${JSON.stringify(f.args.account)}`));
    assert.match(script, /startTransaction\(\{readConcern:\{level:'snapshot'\}\}\)/);
    assert.match(script, /session.getDatabase\(db.getName\(\)\)/);
    assert.match(script, /finally \{ try \{ session.abortTransaction\(\); \} finally \{ session.endSession\(\); \} \}/);
    assert(!script.includes('commitTransaction'));
    assert.equal(options.timeout, 60_000);
    return JSON.stringify({ accountMatches: 1, itemMatches: 1, state: f.state });
  };
  const result = getSavedMongoDbCheckoutState({ ...f.args, exec });
  assert.equal(queries, 1); assert.deepEqual(result.state, f.state);
  assert.deepEqual(result.schemaSha256, { source: f.sourceSha256, reader: f.args.reader.sha256 });
  assert.equal(result.scope, 'orders');
});

test('isolated Mongo reads authenticate with private lease authority', t => {
  const f = fixture(); t.after(() => rmSync(f.root, { recursive: true, force: true }));
  const lease = { ...f.args.lease, ownershipToken: 'private-authority',
    resources: { ...f.args.lease.resources, network: {} as BackendLeaseNetwork } };
  const exec: TextCommandExecutor = (_file, args) => {
    if (args[0] === 'inspect') return 'owned-id';
    const identity = attemptDatabaseIdentity(lease.ownershipToken);
    assert(args.includes(identity.user)); assert(args.includes(identity.password));
    assert.equal(args[args.indexOf('--authenticationDatabase') + 1], 'owned-db');
    return JSON.stringify({ accountMatches: 1, itemMatches: 1, state: f.state });
  };
  getSavedMongoDbCheckoutState({ ...f.args, lease, exec });
  assert.throws(() => getSavedMongoDbCheckoutState({ ...f.args,
    lease: { ...lease, ownershipToken: '' }, exec }), /private lease authority/);
});

test('database errors retain stderr without exposing credentials or the evaluated program', t => {
  const f = fixture(); t.after(() => rmSync(f.root, { recursive: true, force: true }));
  const lease = { ...f.args.lease, ownershipToken: 'private-authority',
    resources: { ...f.args.lease.resources, network: {} as BackendLeaseNetwork } };
  const password = attemptDatabaseIdentity(lease.ownershipToken).password;
  const exec: TextCommandExecutor = (_file, args) => {
    if (args[0] === 'inspect') return 'owned-id';
    throw Object.assign(new Error(`command --password ${password} --eval ${args.at(-1)}`), {
      stderr: `MongoServerError: snapshot unavailable ${password}\n${args.at(-1)}`,
    });
  };
  assert.throws(() => getSavedMongoDbCheckoutState({ ...f.args, lease, exec }), error => {
    assert(error instanceof Error);
    assert.match(error.message, /snapshot unavailable/);
    assert(!error.message.includes(password));
    assert(!error.message.includes('startTransaction'));
    assert(!error.message.includes('--eval'));
    return true;
  });
});
