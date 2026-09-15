import assert from 'node:assert/strict';
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { applyAddressBookMigration, applyAddressBookDefect, ADDRESS_BOOK_DEFECTS } from '../src/references/address-book-migration.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { verifyCheckoutSchema } from '../src/stacks/checkout-state.js';

test('migration delta requires an exact disposable starting source and keeps the registered reference intact', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-migration-'));
  const reference = join(STACK_BENCH_ROOT, 'reference-apps/ecommerce/postgres');
  const original = hashAppSource(reference);
  try {
    cpSync(reference, root, { recursive: true });
    assert.throws(() => applyAddressBookMigration(root, 'unsupported'), /not implemented/);
    const identity = applyAddressBookMigration(root, 'postgres');
    assert.notEqual(identity.sha256, original.sha256);
    assert.equal(hashAppSource(reference).sha256, original.sha256);
    assert(readFileSync(join(root, 'server/src/index.ts'), 'utf8').includes('await initializeAddressBook(pool)'));
    assert(readFileSync(join(root, 'client/src/ProgressionPanel.tsx'), 'utf8').includes('<AddressBook actions='));
    assert.throws(() => applyAddressBookMigration(root, 'postgres'), /exact recorded/);
    // The starting source is checked before any delta is written.
    cpSync(reference, root, { recursive: true });
    writeFileSync(join(root, 'server/src/schema.ts'), '// changed schema\n');
    const changed = hashAppSource(root).sha256;
    assert.throws(() => applyAddressBookMigration(root, 'postgres'), /exact recorded/);
    assert.equal(hashAppSource(root).sha256, changed);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('native schema mapping accepts only the exact address-book delta, with explicit opt-in', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-native-migration-'));
  const file = 'backend/spacetimedb/src/schema.ts';
  try {
    cpSync(join(STACK_BENCH_ROOT, 'reference-apps/ecommerce/spacetime'), root, { recursive: true });
    applyAddressBookMigration(root, 'spacetime');
    assert.throws(() => verifyCheckoutSchema('spacetime', root, [file]), /verified mapping/);
    assert.match(verifyCheckoutSchema('spacetime', root, [file], { addressBookMigration: true })[file]!, /^[a-f0-9]{64}$/);
    const schema = readFileSync(join(root, file), 'utf8');
    const changed = schema.replace('price: t.f64()', 'price: t.i64()');
    assert.notEqual(changed, schema);
    writeFileSync(join(root, file), changed);
    assert.throws(() => verifyCheckoutSchema('spacetime', root, [file], { addressBookMigration: true }), /verified mapping/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('each declared defect changes the migrated source through an exact anchor', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-migration-controls-'));
  try {
    for (const backend of ['postgres', 'mongodb', 'spacetime']) for (const defect of ADDRESS_BOOK_DEFECTS) {
      const app = join(root, backend, defect);
      cpSync(join(STACK_BENCH_ROOT, 'reference-apps/ecommerce', backend), app, { recursive: true });
      const migrated = applyAddressBookMigration(app, backend);
      assert.notEqual(applyAddressBookDefect(app, backend, defect).sha256, migrated.sha256, defect);
      assert.throws(() => applyAddressBookDefect(app, backend, 'unknown'), /unknown/);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
