import assert from 'node:assert/strict';
import test from 'node:test';
import { addressImportDifferences, migrationCheckoutDifferences }
  from '../src/stacks/migration-state.js';
import type { CheckoutState } from '../src/stacks/checkout-state.js';

function population(): CheckoutState[] {
  return [{ accountId: 'customer-1', itemId: 'item-1', priceMinor: 1234,
    cart: [{ itemId: 'item-2', quantity: 1 }], reservations: [],
    stock: [{ warehouseId: 'east', quantity: 7 }, { warehouseId: 'west', quantity: 2 }],
    orders: [{ id: 'order-1', accountId: 'customer-1', totalMinor: 17800, status: 'shipped',
      lines: [{ itemId: 'item-1', quantity: 2, priceMinor: 8900,
        allocations: [{ warehouseId: 'east', quantity: 1 }, { warehouseId: 'west', quantity: 1 }] }] },
    { id: 'order-2', accountId: 'customer-2', totalMinor: 317, status: 'pending',
      lines: [{ itemId: 'item-2', quantity: 1, priceMinor: 317, allocations: [] }] }],
    payments: [{ id: 'payment-1', orderId: 'order-1', amountMinor: 17800, status: 'paid' },
      { id: 'payment-2', orderId: 'order-2', amountMinor: 317, status: 'paid' }],
    orphanOrderLines: 0 }];
}

test('migration preserves selected business records independent of query ordering', () => {
  const before = population();
  const after = structuredClone(before);
  after[0]!.stock.reverse();
  after[0]!.orders[0]!.lines[0]!.allocations.reverse();
  after[0]!.orders.reverse();
  after[0]!.payments.reverse();
  assert.deepEqual(migrationCheckoutDifferences(before, after), []);
  assert.deepEqual(after[0]!.orders.map(order => order.id), ['order-2', 'order-1'], 'comparison must not mutate input');
});

test('migration detects losses, duplicates and changes even when totals stay equal', () => {
  const defects: Record<string, (state: CheckoutState) => void> = {
    lostOrder: state => { state.orders.pop(); },
    duplicateOrder: state => { state.orders.push(structuredClone(state.orders[0]!)); },
    wrongOwner: state => { state.orders[0]!.accountId = 'customer-2'; },
    rewrittenHistoricalPrice: state => { state.orders[0]!.lines[0]!.priceMinor = state.priceMinor; },
    rewrittenCurrentPrice: state => { state.priceMinor = 8900; },
    changedOrderId: state => { state.orders[0]!.id = 'replacement'; },
    changedStatus: state => { state.orders[0]!.status = 'pending'; },
    lostPayment: state => { state.payments.pop(); },
    changedPaymentAmount: state => { state.payments[0]!.amountMinor++; },
    wrongPaymentOwner: state => { state.payments[0]!.orderId = 'order-2'; },
    balancedStockCorruption: state => { state.stock[0]!.quantity--; state.stock[1]!.quantity++; },
    changedWarehouse: state => { state.stock[0]!.warehouseId = 'elsewhere'; },
    lostCart: state => { state.cart = []; },
    createdReservation: state => { state.reservations.push({ itemId: 'item-1', warehouseId: 'east', quantity: 1 }); },
    orphanLine: state => { state.orphanOrderLines++; },
    wrongAccountScope: state => { state.accountId = 'another-account'; },
    wrongItemScope: state => { state.itemId = 'another-item'; },
  };
  for (const [name, defect] of Object.entries(defects)) {
    const before = population(), after = structuredClone(before);
    defect(after[0]!);
    assert(migrationCheckoutDifferences(before, after).length > 0, name);
  }
  assert(migrationCheckoutDifferences(population(), []).length > 0, 'missing scope');
});

function imports() {
  const before = [
    { accountId: 'a', name: 'Avery', address: ' 12 Café Street\nApt 2 ' },
    { accountId: 'b', name: 'Avery', address: ' 12 Café Street\nApt 2 ' },
    { accountId: 'empty', name: '', address: '' },
  ];
  const after = before.map(value => ({ accountId: value.accountId,
    entries: value.name || value.address
      ? [{ id: `opaque-${value.accountId}`, name: value.name, address: value.address, isDefault: true }] : [],
    legacyProfile: { name: value.name, address: value.address },
  }));
  return { before, after };
}

test('address imports preserve exact text and separate owners with equal addresses', () => {
  const { before, after } = imports();
  after.reverse();
  assert.deepEqual(addressImportDifferences(before, after), []);
  // No UUID, integer ID, physical table, or eager conversion requirement.
  after.find(book => book.accountId === 'a')!.entries[0]!.id = '18446744073709551615';
  assert.deepEqual(addressImportDifferences(before, after), []);
  assert.deepEqual(addressImportDifferences([{ accountId: 'x', name: 'Name only', address: '' }],
    [{ accountId: 'x', entries: [{ id: '1', name: 'Name only', address: '', isDefault: true }],
      legacyProfile: { name: 'Name only', address: '' } }]), []);
});

test('import observer detects no-op, wrong owner, duplicate, altered text and stale legacy profile', () => {
  const defects: Record<string, (state: ReturnType<typeof imports>['after']) => void> = {
    noImport: state => { state[0]!.entries = []; },
    wrongOwner: state => { state[0]!.accountId = 'intruder'; },
    duplicateBook: state => { state.push(structuredClone(state[0]!)); },
    missingOwner: state => { state.pop(); },
    duplicateEntry: state => { state[0]!.entries.push(structuredClone(state[0]!.entries[0]!)); },
    trimmedText: state => { state[0]!.entries[0]!.address = state[0]!.entries[0]!.address.trim(); },
    changedName: state => { state[0]!.entries[0]!.name = 'Someone else'; },
    noDefault: state => { state[0]!.entries[0]!.isDefault = false; },
    staleProfile: state => { state[0]!.legacyProfile.address = ''; },
    emptyProfileImported: state => { state[2]!.entries.push({ id: 'empty', name: '', address: '', isDefault: true }); },
  };
  for (const [name, defect] of Object.entries(defects)) {
    const { before, after } = imports();
    defect(after);
    assert(addressImportDifferences(before, after).length > 0, name);
  }
  const { before } = imports();
  assert(addressImportDifferences(before, []).length > 0);
});

test('invalid or ambiguous observation inputs throw instead of passing or inventing an app defect', () => {
  for (const value of [null, {}, undefined, '[]']) {
    assert.throws(() => migrationCheckoutDifferences(population(), value));
    assert.throws(() => addressImportDifferences(imports().before, value));
  }
  assert.throws(() => migrationCheckoutDifferences([], []), 'empty baseline');
  const emptyStore = population();
  emptyStore[0]!.orders = []; emptyStore[0]!.payments = [];
  assert.throws(() => migrationCheckoutDifferences(emptyStore, emptyStore), /measured orders/);
  assert.throws(() => addressImportDifferences([], []), 'empty baseline');
  const duplicated = [...population(), ...population()];
  assert.throws(() => migrationCheckoutDifferences(duplicated, duplicated), /duplicate/);
  const { before, after } = imports();
  assert.throws(() => addressImportDifferences([...before, before[0]], after), /duplicate/);
  const invalid = population();
  invalid[0]!.priceMinor = 1.234;
  assert.throws(() => migrationCheckoutDifferences(population(), invalid));
  delete (after[0] as Partial<typeof after[0]>)!.legacyProfile;
  assert.throws(() => addressImportDifferences(before, after));
});
