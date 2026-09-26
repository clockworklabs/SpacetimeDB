import assert from 'node:assert/strict';
import test from 'node:test';
import type { BackendLease } from '../src/runtime/backend-lease.js';
import type { TextCommandExecutor } from '../src/runtime/command-executor.js';
import { getConvexStock, setConvexStock, getConvexCheckoutState, readConvexTables, probeConvexNamedAction } from '../src/stacks/backends/convex-operations.js';
import { classifyResponseContract } from '../src/actions/named-action-runtime.js';

const lease = { backend: 'convex', resources: { serverUri: 'http://127.0.0.1:13210',
  container: { id: 'owned', name: 'owned-name' }, network: { namespaceContainerId: 'owned',
    namespaceStartedAt: 'start', firewallSha256: 'firewall', firewallInstalledAt: 'installed' } } } as BackendLease;
const rows = () => ({
  item: [{ _id: 'item-native', name: 'Keyboard', price: 19.99 }],
  warehouse: [{ _id: 'warehouse-native', name: 'Main' }],
  stock: [{ _id: 'stock-native', item_id: 'item-native', warehouse_id: 'warehouse-native', quantity: 3 }],
  order_account: [{ _id: 'account-native', username: 'buyer' }],
  order_header: [], order_line: [], order_cart: [], order_reservation: [], order_allocation: [],
});
function native(data: Record<string, Record<string, unknown>[]>, options: { stall?: boolean; namespaceStarted?: string; omitTable?: string; noTruncate?: boolean; noTimestamp?: boolean } = {}) {
  const calls: Record<string, unknown>[] = [];
  const exec: TextCommandExecutor = (command, args, commandOptions) => {
    assert(!args.some(arg => arg.includes('private-admin')), 'admin key must never enter argv');
    if (args[0] === 'inspect') return JSON.stringify({ Id: 'owned',
      State: { Running: true, StartedAt: options.namespaceStarted ?? 'start' } });
    if (args.includes('./generate_admin_key.sh')) return 'private-admin';
    assert(command === 'curl' || args.includes('curl'));
    const config = commandOptions.input!;
    assert(config.includes('Authorization: Convex private-admin'));
    const body = JSON.parse(JSON.parse(config.split('\n').find(line => line.startsWith('data = '))!.slice(7)));
    calls.push(body);
    if (body.path === '_system/cli/tables') return JSON.stringify({ status: 'success', value: {
      page: Object.keys(data).map(name => ({ name })), isDone: true } });
    if (body.path === '_system/cli/modules:apiSpec') return JSON.stringify({ status: 'success', value: [
      { identifier: 'api.js:checkout', functionType: 'Mutation', visibility: { kind: 'public' } },
      { identifier: 'api.js:private', functionType: 'Mutation', visibility: { kind: 'internal' } },
      { identifier: 'api.js:query', functionType: 'Query', visibility: { kind: 'public' } },
    ] });
    if (body.path === '_system/frontend/patchDocumentsFields') {
      assert.deepEqual(body.args, { table: 'stock', ids: ['stock-native'], fields: { quantity: 17 }, componentId: null });
      data.stock![0]!.quantity = 17;
      return JSON.stringify({ status: 'success', value: { success: true } });
    }
    const names = Object.keys(body.selection['']).filter(name => name !== '_other' && name !== options.omitTable);
    return JSON.stringify({ status: options.stall ? { type: 'snapshotting' } : { type: 'upToDate', ...(options.noTimestamp ? {} : { snapshotTs: 123 }) },
      truncates: options.noTruncate ? [] : names.map(table => ({ component: '', table })),
      values: names.flatMap(table => data[table]!.map(value => ({ component: '', table, deleted: false, value }))),
      pagination: { nextCursor: options.stall ? 'same-cursor' : null } });
  };
  return { exec, calls };
}

test('native stock observer reads stored rows and patches only the selected native document', () => {
  const data = rows(), client = native(data);
  assert.deepEqual(getConvexStock({ item: 'Keyboard', warehouse: 'Main', lease, exec: client.exec }),
    { backend: 'convex', item: 'Keyboard', warehouse: 'Main', quantity: 3 });
  setConvexStock({ item: 'Keyboard', warehouse: 'Main', quantity: 17, lease, exec: client.exec });
  assert.equal(data.stock[0]!.quantity, 17);
  assert(!client.calls.some(call => String(call.path).startsWith('api:')), 'never use an app observer');
});

test('native order snapshots normalize only IDs and use the shared order oracle', () => {
  const data = rows(), client = native(data);
  const result = getConvexCheckoutState({ account: 'buyer', item: 'Keyboard', lease, exec: client.exec,
    storage: { kind: 'order-data', cart: true, warehouses: true } });
  assert.equal(result.state.accountId, 'buyer');
  assert.equal(result.state.priceMinor, 1999);
  assert.equal(result.state.stock[0]!.quantity, 3);
  // A diagnostic read without a storage selection is the caller's error, not an invalid app interface.
  assert.throws(() => getConvexCheckoutState({ account: 'buyer', item: 'Keyboard', lease, exec: client.exec }),
    (error: unknown) => error instanceof Error && /declared order data/.test(error.message)
      && !('orderDataInterface' in error));
});

test('native observer rejects missing tables, duplicate links, invalid quantities and stalled snapshots', () => {
  for (const defect of ['missing', 'duplicate', 'quantity', 'link']) {
    const data: Record<string, Record<string, unknown>[]> = rows();
    if (defect === 'missing') delete data.stock;
    if (defect === 'duplicate') data.stock!.push({ ...data.stock![0], _id: 'another' });
    if (defect === 'quantity') data.stock![0]!.quantity = 0.5;
    if (defect === 'link') data.stock![0]!.warehouse_id = 'orphan';
    assert.throws(() => getConvexStock({ item: 'Keyboard', lease, exec: native(data).exec }),
      (error: unknown) => (error as { stockInterface?: boolean }).stockInterface === true, defect);
  }
  const nestedTimestamp = rows(); Object.assign(nestedTimestamp.item[0]!, { snapshotTs: 456 });
  assert.throws(() => readConvexTables(['item'], { lease, exec: native(nestedTimestamp, { noTimestamp: true }).exec }), /consistency timestamp/);
  assert.throws(() => readConvexTables(['order_cart'], { lease, exec: native(rows(), { omitTable: 'order_cart' }).exec }), /omitted a selected table/);
  assert.throws(() => readConvexTables(['item'], { lease, exec: native(rows(), { noTruncate: true }).exec }), /invalid document/);
  assert.throws(() => readConvexTables(['item'], { lease, exec: native(rows(), { stall: true }).exec }), /cursor did not advance/);
  assert.throws(() => getConvexStock({ item: 'Keyboard', lease, exec: native(rows(), { namespaceStarted: 'changed' }).exec }), /namespace anchor/);
});

test('native envelopes preserve HTTP status semantics without turning errors into acceptance or refusals', () => {
  const request = { responseContract: 'convex-mutation' as const };
  for (const status of [200, 560]) {
    const rejected = classifyResponseContract(request, { status, text: JSON.stringify({ status: 'error', errorMessage: 'no', errorData: null }) });
    assert.equal(rejected.ok, false); assert.equal(rejected.applicationRejected, true);
  }
  for (const [status, text] of [[400, 'bad args'], [200, '{"status":"error","errorMessage":"missing function"}']] as const) {
    const result = classifyResponseContract(request, { status, text });
    assert.equal(result.ok, false); assert.equal(result.refusalKind, null);
  }
  assert.equal(classifyResponseContract(request, { status: 200, text: '{}' }).complete, true);
  assert.equal(classifyResponseContract(request, { status: 200, text: '{}' }).ok, false);
  assert.equal(classifyResponseContract(request, { status: 200, text: '{"status":"success","value":null}' }).ok, true);
});


test('native presence diagnostic uses vendor metadata and cannot treat a private, query or missing export as a mutation', () => {
  const client = native(rows());
  for (const reducer of ['checkout', 'private', 'query', 'missing']) {
    const result = probeConvexNamedAction({ reducer }, { lease, exec: client.exec });
    assert.equal(result.ok, reducer === 'checkout');
  }
  assert(client.calls.every(call => call.path === '_system/cli/modules:apiSpec'));
});
