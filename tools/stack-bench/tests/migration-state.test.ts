import assert from 'node:assert/strict';
import test from 'node:test';
import { addressImportDifferences, migrationCheckoutDifferences }
  from '../src/stacks/migration-state.js';
import type { CheckoutState } from '../src/stacks/checkout-state.js';
import { ADDRESS_BOOK_ACTION_IMPLEMENTATIONS } from '../src/actions/address-book-action-executors.js';
import { ActionApplicationFailure } from '../src/actions/action-contract.js';
import { readAddressBook } from '../src/stacks/address-book-read.js';
import { ReceivedTransport } from '../grader/transport-frames.js';
import { ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS } from '../src/actions/actor-transport-action-executors.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';

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

test('address-book observations ignore row and object key order but detect changed identities and duplicates', async () => {
  let entries = [
    { id: 'opaque-1', name: 'Avery', address: ' 12 Café Street\nApt 2 ', isDefault: true },
    { id: 'opaque-2', name: 'Other', address: '34 Oak Street', isDefault: false },
  ];
  const recorded = new Map<string, unknown>();
  const capabilities = {
    actors: { get: () => ({ name: 'owner', writes: [{ headers: { Authorization: 'Bearer owner' } }] }) },
    'address-book-read': { read: async () => ({ status: 200, text: '', entries: structuredClone(entries) }) },
    'browser-observation': { recorded },
  };
  const run = (action: keyof typeof ADDRESS_BOOK_ACTION_IMPLEMENTATIONS, input: Record<string, unknown>) =>
    ADDRESS_BOOK_ACTION_IMPLEMENTATIONS[action]({ input: { actor: 'owner', ...input }, capabilities,
      signal: new AbortController().signal });
  await run('recordAddressBook', { as: 'before' });
  await run('recordAddressBook', { as: 'home', entryName: 'Avery' });
  await run('recordAddressBook', { as: 'other', entryName: 'Other' });
  entries.reverse();
  await run('expectAddressBook', { sameAs: 'before' });
  const expected = entries.map(({ name, address, isDefault }) => ({ isDefault, address, name }));
  await run('expectAddressBook', { entries: expected });
  const bound = expected.map(entry => ({ ...entry, idFrom: entry.name === 'Avery' ? 'home' : 'other' }));
  await run('expectAddressBook', { entries: bound });
  [entries[0]!.id, entries[1]!.id] = [entries[1]!.id, entries[0]!.id];
  await assert.rejects(async () => run('expectAddressBook', { entries: bound }), ActionApplicationFailure);
  [entries[0]!.id, entries[1]!.id] = [entries[1]!.id, entries[0]!.id];
  entries[0]!.id = 'replacement';
  await assert.rejects(async () => run('expectAddressBook', { sameAs: 'before' }), ActionApplicationFailure);
  await assert.rejects(async () => run('expectAddressBook', { entries: bound }), ActionApplicationFailure);
  entries[0]!.id = entries[1]!.id;
  await assert.rejects(async () => run('expectAddressBook', { entries: expected }), ActionApplicationFailure);
  entries[0]!.id = 'opaque-2';
  entries[1]!.address = entries[1]!.address.trim();
  await assert.rejects(async () => run('expectAddressBook', { entries: expected }), ActionApplicationFailure);
});

test('compiled address-book actions require complete expectations and run through the shared registry', async () => {
  const action = ACTION_REGISTRY.get('expectAddressBook');
  for (const input of [{}, { sameAs: 'before', entries: [] }, { entries: [{ name: 'x', address: 'y' }] },
    { entries: [{ name: 'x', address: 'y', isDefault: true, extra: 1 }] }, { entries: [], authentication: 'publisher' }]) {
    assert.throws(() => action.compile({ do: 'expectAddressBook', actor: 'owner', ...input }), /invalid benchmark definition/);
  }
  const result = await executeAction(ACTION_REGISTRY, 'expectAddressBook', {
    do: 'expectAddressBook', actor: 'owner', entries: [],
  }, { capabilities: {
    actors: { get: () => ({ name: 'owner', writes: [{ headers: { Authorization: 'Bearer owner' } }], record() {} }) },
    'address-book-read': { read: async () => ({ status: 200, text: '{"entries":[]}', entries: [] }) },
    'browser-observation': { recorded: new Map() },
  } });
  assert.equal(result.status, 'passed');
});

test('native address reads use actor auth and typed opaque IDs; failed or malformed reads never become empty books', async () => {
  const signal = new AbortController().signal;
  const captured: string[] = [];
  const native = { backend: 'spacetime', url: 'http://ui.test', spacetime: { uri: 'http://db.test', mod: 'store' } };
  const result = await readAddressBook(native, { Authorization: 'Bearer owner' }, signal, text => captured.push(text),
    async (url, options) => {
      assert.equal(url, 'http://db.test/v1/database/store/sql');
      assert.equal(options?.method, 'POST');
      assert.equal(options?.body, 'SELECT id, name, address, is_default FROM my_addresses');
      assert.equal(new Headers(options?.headers).get('Authorization'), 'Bearer owner');
      assert.equal(options?.signal, signal);
      return new Response(JSON.stringify([{ rows: [['uuid:not-a-number', 'Owner', ' exact text ', true]] }]));
    });
  assert.equal(result.entries?.[0]?.id, 'uuid:not-a-number');
  for (const response of [new Response('offline', { status: 503 }), new Response('not JSON'),
    new Response(JSON.stringify([{ rows: [[1, 'Owner', 'Address', true]] }])), new Response('{}')]) {
    await assert.rejects(readAddressBook(native, {}, signal, text => captured.push(text), async () => response));
  }
  assert.equal(captured.length, 5, 'failed response bodies are captured before interpretation');
});

test('address-book privacy checks inspect complete controller responses, including refusals and extra fields', async () => {
  for (const status of [200, 401, 403]) {
    const received = new ReceivedTransport();
    const actor = { name: 'guest', record: (text: string) => received.record(text),
      wasSent: (needle: string) => received.contains(needle) };
    const signal = new AbortController().signal;
    const capabilities = {
      actors: { get: () => actor },
      'address-book-read': { read: (credentials: Record<string, string>, cancellation: AbortSignal,
        capture: (text: string) => void) => readAddressBook({ backend: 'postgres', url: 'http://app.test' },
        credentials, cancellation, capture, async (_url, options) => {
          assert.equal(options?.body, undefined, 'ordinary GET has no body');
          return new Response(JSON.stringify({ entries: [], debug: 'other-owner-private-address' }), { status });
        }) },
      'transport-observation': { expand: (value: string) => value, sleep: async () => {} },
    };
    await ADDRESS_BOOK_ACTION_IMPLEMENTATIONS.expectAddressBook({
      input: { actor: 'guest', authentication: 'none', entries: [] }, capabilities, signal,
    });
    await assert.rejects(async () => ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS.expectNotReceived({
      input: { actor: 'guest', contains: 'other-owner-private-address', within: 1 }, capabilities, signal,
    }), ActionApplicationFailure);
  }
});
