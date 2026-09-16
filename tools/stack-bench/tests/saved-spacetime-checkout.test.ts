import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';
import { getSavedSpacetimeCheckoutState } from '../src/stacks/backends/saved-spacetime-checkout.js';

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'saved-spacetime-')), app = join(root, 'app');
  mkdirSync(app); writeFileSync(join(app, 'module.ts'), 'accepted');
  const state = { accountId: '1', itemId: '2', priceMinor: 8900, cart: [],
    stock: [{ warehouseId: '3', quantity: 4 }], orders: [], payments: [], reservations: [],
    orphanOrderLines: 0, orphanAllocations: 0 };
  const mapping = { sourceSha256: hashAppSource(app).sha256, tables: ['account', 'item', 'stock'],
    convert: `(tables, account, item) => ({accountMatches: tables.account.length, itemMatches: tables.item.length, state: ${JSON.stringify(state)}})` };
  const reader = { path: join(root, 'reader.json'), sha256: '' };
  const save = () => { const bytes = JSON.stringify(mapping); writeFileSync(reader.path, bytes); reader.sha256 = createHash('sha256').update(bytes).digest('hex'); };
  save();
  const snapshot = { account: { inserts: [{ id: 1 }], deletes: [] }, item: { inserts: [{ id: 2 }], deletes: [] } };
  return { root, mapping, save, state, snapshot, args: { app, reader, account: 'buyer', item: 'Keyboard',
    spacetime: { buildContainer: { id: 'owned-id', name: 'owned-name' }, mod: 'owned-module', containerUri: 'http://127.0.0.1:3211' } } };
}

test('saved SpacetimeDB reader collects all tables in one bounded initial native subscription', t => {
  const f = fixture(); t.after(() => rmSync(f.root, { recursive: true, force: true }));
  let reads = 0;
  const exec: TextCommandExecutor = (file, args, options) => {
    assert.equal(file, 'docker');
    if (args[0] === 'inspect') { assert.equal(args.at(-1), 'owned-name'); return 'owned-id'; }
    reads++; assert(args.includes('owned-id')); assert(args.includes('subscribe')); assert(args.includes('owned-module'));
    assert(args.includes('--print-initial-update')); assert(args.includes('--num-updates')); assert(args.includes('0'));
    assert.deepEqual(args.slice(-3), ['SELECT * FROM account', 'SELECT * FROM item', 'SELECT * FROM stock']);
    assert.equal(options.timeout, 60_000); return JSON.stringify(f.snapshot);
  };
  const result = getSavedSpacetimeCheckoutState({ ...f.args, exec });
  assert.equal(reads, 1); assert.deepEqual(result.state, f.state); assert.equal(result.scope, 'orders');
  assert.deepEqual(result.schemaSha256, { source: f.mapping.sourceSha256, reader: f.args.reader.sha256 });
});

test('saved snapshot source, reader and owned container bindings fail closed', t => {
  const f = fixture(); t.after(() => rmSync(f.root, { recursive: true, force: true }));
  const noExec: TextCommandExecutor = () => { throw new Error('must not execute'); };
  assert.throws(() => getSavedSpacetimeCheckoutState({ ...f.args, reader: { ...f.args.reader, sha256: '0'.repeat(64) }, exec: noExec }), /reader hash mismatch/);
  writeFileSync(join(f.args.app, 'module.ts'), 'changed');
  assert.throws(() => getSavedSpacetimeCheckoutState({ ...f.args, exec: noExec }), /source hash mismatch/);
  writeFileSync(join(f.args.app, 'module.ts'), 'accepted');
  assert.throws(() => getSavedSpacetimeCheckoutState({ ...f.args, exec: () => 'replacement' }), /changed after lease creation/);
});

test('saved snapshot rejects malformed tables, missing entities, invalid canonical state and a stuck converter', t => {
  const f = fixture(); t.after(() => rmSync(f.root, { recursive: true, force: true }));
  const read = (value: unknown) => getSavedSpacetimeCheckoutState({ ...f.args,
    exec: (_file, args) => args[0] === 'inspect' ? 'owned-id' : JSON.stringify(value) });
  for (const value of [{ ...f.snapshot, alien: { inserts: [], deletes: [] } },
    { ...f.snapshot, account: { inserts: [], deletes: [] } },
    { ...f.snapshot, item: { inserts: [{ id: 1 }, { id: 2 }], deletes: [] } },
    { ...f.snapshot, stock: { inserts: [], deletes: [{}] } }]) assert.throws(() => read(value));
  f.mapping.convert = '() => ({accountMatches:1,itemMatches:1,state:{}})'; f.save();
  assert.throws(() => read(f.snapshot));
  f.mapping.convert = '() => { while (true) {} }'; f.save();
  assert.throws(() => read(f.snapshot), /timed out/);
});
