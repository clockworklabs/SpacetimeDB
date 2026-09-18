import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { openCrashReducerConnection } from '../src/stacks/spacetime-crash-transport.js';
import { executeAction } from '../src/actions/action-contract.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { checkoutStateSchema, type CheckoutState } from '../src/stacks/checkout-state.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('baseline checkout retains completion evidence and cannot confirm queued or lost responses', async () => {
  for (const status of [200, 201, 204, 202, 409, 500, null]) {
    let unsettled = false;
    const result = await executeAction(ACTION_REGISTRY, 'confirmCheckout', {
      do: 'confirmCheckout', actor: 'buyer',
    }, { capabilities: {
      actors: { get: () => ({ name: 'buyer', context: { cookies: async () => [] }, writes: [{ url: 'http://app/session', headers: { authorization: 'Bearer private-token' } }] }) },
      'database-read': { markCheckoutUnsettled: () => { unsettled = true; } },
      'named-actions': { now: Date.now, resolve: () => ({ id: 'checkout' }),
        request: () => ({ url: 'http://app/checkout', method: 'POST' }), fetch: async () => {
          if (status === null) throw new Error('response lost');
          return { ok: status < 300, status, text: async () => 'private response body' };
        } },
    } });
    const confirmed = status !== null && [200, 201, 204].includes(status);
    assert.equal(result.status, confirmed ? 'passed' : status === null || status === 202 ? 'inconclusive' : 'failed');
    assert.equal(unsettled, !confirmed);
    const receipt = result.observation as { outcome: string; protocol: string; actor: string; action: string };
    assert.equal(receipt.outcome, confirmed ? 'committed' : 'not-confirmed');
    assert.equal(receipt.protocol, 'http');
    assert.equal(receipt.actor, 'buyer');
    assert.equal(receipt.action, 'checkout');
    assert(!JSON.stringify(result).includes('private'));
  }
});

test('every draft crash baseline confirms then reconciles checkout before reloading', () => {
  for (const boundary of ['application', 'database']) {
    const scenario = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
      `tracks/ecommerce/scenarios/diagnostic-checkout-${boundary}-crash.json`), 'utf8'));
    for (const feature of scenario.features) {
      const steps = feature.setup as Array<{ do: string; before?: string; prepared?: string; testid?: string }>;
      const index = steps.findIndex(step => step.do === 'confirmCheckout');
      assert(index > 0);
      assert.equal(steps[index - 1]!.do, 'dbRecordCheckout');
      assert.deepEqual(steps.slice(index + 1, index + 3).map(step => step.do), ['dbExpectCheckout', 'reload']);
      assert.equal(steps[index + 1]!.before, 'baseline-before');
      assert.equal(steps[index + 1]!.prepared, 'baseline-prepared');
      assert(!steps.some(step => step.testid === 'checkout-submit'));
      assert(feature.criteria.every((criterion: { points: number }) => criterion.points === 0));
    }
  }
});

class Socket extends EventTarget {
  protocol = 'v1.json.spacetimedb';
  readyState: number = WebSocket.OPEN;
  sent: Array<{ CallReducer: { reducer: string; request_id: number; args: string; flags: number } }> = [];
  send(value: string) { this.sent.push(JSON.parse(value)); }
  close() { this.readyState = WebSocket.CLOSED; }
  message(value: unknown) {
    const data = JSON.stringify(value).replace(/"(22451491060779870000000000000000000000[12])"/g, '$1');
    this.dispatchEvent(Object.assign(new Event('message'), { data }));
  }
  reply(requestId: number, status: object, caller = `0x${'1'.repeat(64)}`,
    connection = '224514910607798700000000000000000000001') {
    this.message({ TransactionUpdate: { caller_identity: { __identity__: caller },
      caller_connection_id: { __connection_id__: connection }, timing: 1.5,
      reducer_call: { request_id: requestId, reducer_name: 'checkout' }, status } });
  }
}

test('baseline SpacetimeDB checkout records native completion or refusal and closes its connection', async t => {
  const websocket = globalThis.WebSocket;
  t.after(() => { globalThis.WebSocket = websocket; });
  for (const committed of [true, false]) {
    const sockets: Socket[] = [];
    globalThis.WebSocket = class extends Socket {
      static OPEN = websocket.OPEN;
      static CLOSED = websocket.CLOSED;
      constructor(address: string, protocol: string) {
        super(); sockets.push(this);
        assert.equal(new URL(address).searchParams.get('confirmed'), 'true');
        assert.equal(protocol, this.protocol);
        queueMicrotask(() => this.message({ IdentityToken: { identity: { __identity__: `0x${'1'.repeat(64)}` },
          connection_id: { __connection_id__: '224514910607798700000000000000000000001' } } }));
      }
      override send(value: string) {
        super.send(value);
        queueMicrotask(() => this.reply(1, committed ? { Committed: {} } : { Failed: 'refused' }));
      }
    } as unknown as typeof WebSocket;
    const result = await executeAction(ACTION_REGISTRY, 'confirmCheckout', {
      do: 'confirmCheckout', actor: 'buyer',
    }, { capabilities: {
      actors: { get: () => ({ name: 'buyer', context: { cookies: async () => [] }, writes: [{ url: 'http://app/session', headers: { authorization: 'Bearer private-token' } }] }) },
      'database-read': { markCheckoutUnsettled: () => assert.fail('native completion or refusal must settle') },
      'named-actions': { spacetime: { uri: 'http://127.0.0.1:3000', mod: 'shop' }, now: Date.now,
        resolve: () => ({ id: 'checkout', reducer: 'checkout' }),
        request: () => ({ url: 'http://app/call/checkout', body: '[]' }),
        fetch: async () => assert.fail('baseline must not use the unconfirmed HTTP reducer path') },
    } });
    assert.equal(result.status, committed ? 'passed' : 'failed');
    assert.equal((result.observation as { protocol: string }).protocol, 'websocket-v1-confirmed');
    assert.equal(sockets.length, 1);
    assert.equal(sockets[0]!.sent.length, 1);
    assert.equal(sockets[0]!.readyState, websocket.CLOSED);
  }
});

test('native crash transport requires a correlated confirmed result and drains unknowns without replay', async () => {
  const socket = new Socket();
  const controller = new AbortController();
  const connection = await openCrashReducerConnection({ uri: 'http://127.0.0.1:3000', mod: 'shop' },
    'private-token', controller.signal, (address, protocol) => {
      const url = new URL(address);
      assert.equal(url.protocol, 'ws:');
      assert.equal(url.searchParams.get('confirmed'), 'true');
      assert.equal(url.searchParams.get('token'), 'private-token');
      assert.equal(protocol, socket.protocol);
      queueMicrotask(() => socket.message({ IdentityToken: { identity: { __identity__: `0x${'1'.repeat(64)}` },
        connection_id: { __connection_id__: '224514910607798700000000000000000000001' } } }));
      return socket as unknown as WebSocket;
    });
  const first = connection.call('checkout', '[]', controller.signal);
  const second = connection.call('checkout', '[]', controller.signal);
  assert.deepEqual(socket.sent[0], { CallReducer: { reducer: 'checkout', args: '[]', request_id: 1, flags: 0 } });
  socket.reply(1, { Committed: {} }, 'another-user');
  socket.reply(1, { Committed: {} }, `0x${'1'.repeat(64)}`, '224514910607798700000000000000000000002');
  socket.reply(2, { Committed: {} });
  controller.abort();
  assert.deepEqual(await first, { outcome: 'unknown' });
  assert.deepEqual(await second, { outcome: 'committed' });
  assert.equal(socket.sent.length, 2);
  assert.equal(socket.readyState, WebSocket.CLOSED);
});

test('crash setup rejects missing cart scope, changed scope and old native reservations before issuing writes', async () => {
  for (const mode of ['missing-cart', 'changed-scope', 'old-reservation']) {
    const before: CheckoutState = { accountId: 'a', itemId: 'i', priceMinor: 100,
      stock: [{ warehouseId: 'w', quantity: 10 }], cart: [], reservations: [], orders: [], payments: [],
      orphanOrderLines: 0, orphanAllocations: 0 };
    const prepared = structuredClone(before);
    prepared.cart.push({ itemId: 'i', quantity: 1 });
    if (mode === 'old-reservation') {
      prepared.stock[0]!.quantity--;
      prepared.reservations.push({ itemId: 'i', warehouseId: 'w', quantity: 1 });
    }
    const wrap = (state: CheckoutState, ready: boolean) => ({ state, schemaSha256: { schema: 'same' },
      account: 'a', item: 'i', scope: 'orders' as const, recordedAtMs: Date.now() - 31_000,
      storage: { kind: 'order-data' as const, cart: mode !== 'missing-cart', warehouses: !(ready && mode === 'changed-scope') } });
    let closed = false;
    const result = await executeAction(ACTION_REGISTRY, 'crashCheckout', {
      do: 'crashCheckout', actor: 'buyer', before: 'before', prepared: 'prepared', quantity: 1,
      requests: 1, offsetMs: 0, target: 'database',
    }, { capabilities: {
      actors: { get: () => ({ name: 'buyer', context: { cookies: async () => [] }, writes: [{ url: 'http://app/session', headers: { authorization: 'Bearer private-token' } }] }) },
      'database-read': { checkoutSnapshots: new Map([['before', wrap(before, false)], ['prepared', wrap(prepared, true)]]) },
      'named-actions': { resolve: () => ({ id: 'checkout' }), request: () => ({ url: 'http://app/checkout' }),
        fetch: async () => assert.fail('invalid crash setup must not submit checkout') },
      'browser-observation': { recorded: new Map() },
      'process-crash': { prepare: async () => {
        assert.equal(mode, 'old-reservation', 'invalid scopes must fail before process preparation');
        return { spacetime: null, close: async () => { closed = true; },
          crash: async () => assert.fail('expired preparation must not inject a fault') };
      } },
    } });
    assert.equal(result.status, mode === 'old-reservation' ? 'inconclusive' : 'harness_failure', mode);
    assert.equal(closed, mode === 'old-reservation');
  }
});

test('crash action retains partial fault evidence and distinguishes recovered state from acknowledged loss', async () => {
  for (const record of [false, true]) for (const mode of ['absent', 'committed', 'orders-only', 'orders-scoped', 'orders-scoped-wrong-total', 'orders-scoped-changed-after', 'orders-scoped-lost-acknowledged', 'empty-stock', 'lost-acknowledged', 'partial', 'queued', 'queued-committed', 'queued-drained', 'queued-recovery-error', 'fault-error', 'cancelled-recovery', 'cancelled-read', 'recovery-error', 'disconnected-database', 'disconnected-application', 'drained-application', 'undrained-application', 'drained-recovery-error', 'disconnected-recovery-error', 'cancelled-disconnected-recovery-error']) {
    const recorded = new Map<string, unknown>(record
      ? [['captured-crash', { verdicts: { atomicity: [], durability: [] } }]] : []);
    const cancellation = new AbortController();
    const timers: number[] = [];
    let recoveryStopped = false;
    let unsettled = false;
    const before: CheckoutState = { accountId: 'a', itemId: 'i', priceMinor: 100,
      stock: [{ warehouseId: 'w', quantity: 10 }], cart: [], reservations: [], orders: [], payments: [], orphanOrderLines: 0 };
    const prepared = structuredClone(before);
    prepared.cart.push({ itemId: 'i', quantity: 1 });
    const after = structuredClone(prepared);
    if (mode === 'committed' || mode === 'partial' || mode === 'orders-only' || mode === 'orders-scoped' || mode === 'orders-scoped-wrong-total' || mode === 'orders-scoped-changed-after' || mode === 'empty-stock' || mode === 'queued-committed') {
      after.cart = []; after.stock[0]!.quantity--;
      after.orders.push({ id: 'o', accountId: 'a', status: 'pending', totalMinor: 100,
        lines: [{ itemId: 'i', quantity: 1, priceMinor: 100, allocations: [{ warehouseId: 'w', quantity: 1 }] }] });
      if (mode === 'committed' || mode === 'empty-stock' || mode === 'queued-committed') after.payments.push({ id: 'p', orderId: 'o', amountMinor: 100, status: 'paid' });
    }
    if (mode === 'empty-stock') after.stock = [];
    if (mode.startsWith('orders-')) for (const state of [before, prepared, after]) {
      state.orphanAllocations = 0;
      if (mode.startsWith('orders-scoped')) {
        state.stock = [];
        for (const order of state.orders) for (const line of order.lines) line.allocations = [];
      }
      for (const order of state.orders) order.refundedMinor = 0;
    }
    if (mode === 'orders-scoped-wrong-total') after.orders[0]!.totalMinor++;
    const wrap = (state: CheckoutState) => ({ state: checkoutStateSchema.parse(state), schemaSha256: { schema: 'same' }, account: 'a', item: 'i',
      ...(mode.startsWith('orders-') ? { scope: 'orders' as const } : {}),
      ...(mode.startsWith('orders-scoped') ? { storage: { kind: 'order-data' as const, cart: true, warehouses: mode === 'orders-scoped-changed-after' && state === after } } : {}),
      recordedAtMs: Date.now() - (mode === 'orders-only' ? 90_000 : 0) });
    let release!: () => void;
    const waiting = new Promise<void>(resolve => { release = resolve; });
    const result = await executeAction(ACTION_REGISTRY, 'crashCheckout', {
      do: 'crashCheckout', actor: 'buyer', before: 'before', prepared: 'prepared', quantity: 1,
      ...(record ? { as: 'captured-crash',
        ...(mode !== 'disconnected-database' ? { reuseCombinedFrom: 'database' } : {}) } : {}),
      requests: 1, offsetMs: 0, target: mode === 'disconnected-database' ? 'database' : 'application',
    }, { signal: cancellation.signal, onAbort: async () => {}, capabilities: {
      actors: { get: () => ({ name: 'buyer', context: { cookies: async () => [] }, writes: [{ url: 'http://app/session', headers: { authorization: 'Bearer private-token' } }] }) },
      'database-read': { checkoutSnapshots: new Map([['before', wrap(before)], ['prepared', wrap(prepared)]]),
        markCheckoutUnsettled: () => { unsettled = true; },
        getCheckoutState: () => { if (mode === 'cancelled-read') throw new Error('reader is not ready'); return wrap(after); } },
      'named-actions': { now: Date.now, sleep: async () => {
        if (mode === 'cancelled-read') { cancellation.abort('cancelled during read retry'); throw cancellation.signal.reason; }
      }, resolve: () => ({ id: 'checkout' }),
        request: () => ({ url: 'http://app/checkout' }), fetch: async () => {
          await waiting;
          if (mode.includes('disconnected-') || mode.endsWith('drained-application')) throw new Error('socket closed');
          if (mode === 'absent' || mode === 'partial') return { ok: false, status: 409, text: async () => '' };
          if (mode.startsWith('queued')) return { ok: true, status: 202, text: async () => '' };
          return { ok: true, status: 200, text: async () => '' };
        } },
      'browser-observation': { recorded },
      'process-crash': { combinedBoundary: false, prepare: async () => ({ spacetime: null,
        close: async () => {},
        crash: async () => {
          const now = Date.now();
          const receipt = { requestedAtMs: now, completedAtMs: now, clockOffsetBeforeMs: 0, clockOffsetAfterMs: 0,
            signal: 'SIGKILL', processEvidence: `KILLED 42 123 ${now}\nQUIET\n` };
          if (mode === 'fault-error') throw Object.assign(new Error('partial injection'), { receipt });
          return receipt;
        }, recover: async (signal: AbortSignal) => {
          if (mode === 'cancelled-recovery' || mode === 'cancelled-disconnected-recovery-error') {
            cancellation.abort('cancelled during recovery');
            assert(signal.aborted, 'recovery must receive action cancellation');
            // Model a worker waiting for an in-flight native call to finish.
            await new Promise(resolve => setTimeout(resolve, 10));
          }
          recoveryStopped = true;
          release();
          if (mode.endsWith('recovery-error')) throw Object.assign(new Error('application did not recover'), {
            code: 'generated_app_not_restartable',
            ...(mode === 'drained-recovery-error' ? { databaseDrain: { settled: true, samples: [{ pending: 0 }] } } : {}),
          });
          return mode.endsWith('drained-application') || mode === 'queued-drained' ? { settled: mode !== 'undrained-application',
            startedAtMs: Date.now(), completedAtMs: Date.now(), backend: 'postgres', database: 'app',
            samples: [{ atMs: Date.now(), pending: mode === 'undrained-application' ? 1 : 0 }] } : null;
        } }) },
    } }, { setTimer: (callback, ms) => {
      timers.push(ms);
      return setTimeout(callback, ms === 5000 ? 1 : ms === 150_000 ? 100 : ms);
    } });
    assert(recoveryStopped, 'outer action must not return before recovery has stopped');
    if (mode.startsWith('cancelled-')) {
      assert(timers.includes(150_000));
      assert.equal(result.code, 'cancelled');
    }
    assert.equal(unsettled, mode.startsWith('queued') || mode.includes('disconnected-') || mode === 'undrained-application');
    const expected = (mode.startsWith('queued') && mode !== 'queued-recovery-error') || mode.startsWith('cancelled-') || ['disconnected-database', 'disconnected-application', 'undrained-application'].includes(mode) ? 'inconclusive' : ['fault-error', 'orders-scoped-changed-after'].includes(mode) ? 'harness_failure'
      : ['partial', 'lost-acknowledged', 'empty-stock', 'orders-scoped-wrong-total', 'orders-scoped-lost-acknowledged'].includes(mode) || mode.endsWith('recovery-error') ? 'failed' : 'passed';
    const capturedFailure = record && ['partial', 'lost-acknowledged', 'empty-stock', 'orders-scoped-wrong-total', 'orders-scoped-lost-acknowledged'].includes(mode);
    assert.equal(result.status, capturedFailure ? 'passed' : expected, mode);
    if (record && result.status === 'passed') {
      const results = [];
      for (const verdict of ['atomicity', 'durability']) {
        results.push(await executeAction(ACTION_REGISTRY, 'expectCrashCheckout', {
          do: 'expectCrashCheckout', from: 'captured-crash', verdict,
        }, { capabilities: { 'browser-observation': { recorded } } }));
      }
      assert.equal(results.some(row => row.status === 'failed'), expected === 'failed', mode);
      assert(results.every(row => ['passed', 'failed'].includes(row.status)), mode);
      assert(results.every(row => row.observation === recorded.get('captured-crash')));
    } else assert.equal(recorded.get('captured-crash'), undefined, 'failed capture must clear the previous verdict');
    if (mode === 'empty-stock') {
      assert.equal(result.status, record ? 'passed' : 'failed');
      assert.deepEqual((result.observation as { after: { state: CheckoutState } }).after.state.stock, []);
    }
    if (mode.startsWith('queued')) {
      const evidence = result.observation as { confirmed: boolean; outcomes: Array<{ outcome: string; status: number }> };
      assert.equal(evidence.confirmed, false);
      assert.deepEqual(evidence.outcomes.map(row => [row.outcome, row.status]), [['not-confirmed', 202]]);
    }
    if (mode === 'disconnected-recovery-error' || mode === 'queued-recovery-error') assert.equal(result.code, 'application_failure');
    if (mode === 'drained-recovery-error') assert.equal((result.observation as { databaseDrain: { settled: boolean } }).databaseDrain.settled, true);
    assert(result.observation, mode);
    assert(!JSON.stringify(result.observation).includes('private-token'));
    assert.doesNotThrow(() => createCheckEvidence({ status: result.status, code: result.code, phase: 'assertion',
      observation: result.observation, actions: [{ actor: 'buyer', evidence: result }],
      startedAtMs: result.timing.startedAtMs, completedAtMs: result.timing.completedAtMs }));
  }
});


test('crash verdict without a measured observation remains inconclusive', async () => {
  for (const verdict of ['atomicity', 'durability']) {
    const result = await executeAction(ACTION_REGISTRY, 'expectCrashCheckout', {
      do: 'expectCrashCheckout', from: 'missing', verdict,
    }, { capabilities: { 'browser-observation': { recorded: new Map() } } });
    assert.equal(result.status, 'inconclusive');
  }
});

test('combined application boundary reuses only a measured database crash and retains its failures', async () => {
  for (const prior of [undefined, { receipt: { backend: 'postgres', target: 'database' }, verdicts: { atomicity: [], durability: [] } },
    { receipt: { backend: 'spacetime', target: 'database' }, verdicts: {
      atomicity: [{ control: 'partial order', observed: 0, expected: 1 }], durability: [] } }]) {
    const recorded = new Map<string, unknown>([['database', prior]]);
    const result = await executeAction(ACTION_REGISTRY, 'crashCheckout', {
      do: 'crashCheckout', actor: 'buyer', before: 'before', prepared: 'prepared', quantity: 1,
      requests: 16, offsetMs: 0, target: 'application', as: 'application', reuseCombinedFrom: 'database',
    }, { capabilities: { actors: {}, 'named-actions': {}, 'database-read': {}, 'browser-observation': { recorded },
      'process-crash': { combinedBoundary: true, prepare: () => assert.fail('must not crash the same process twice') } } });
    if (prior?.receipt.backend === 'spacetime') {
      assert.equal(result.status, 'passed');
      assert.equal(recorded.get('application'), prior);
      const check = await executeAction(ACTION_REGISTRY, 'expectCrashCheckout', {
        do: 'expectCrashCheckout', from: 'application', verdict: 'atomicity',
      }, { capabilities: { 'browser-observation': { recorded } } });
      assert.equal(check.status, 'failed');
    } else {
      assert.equal(result.status, 'inconclusive');
      assert.equal(recorded.get('application'), undefined);
    }
  }
});
