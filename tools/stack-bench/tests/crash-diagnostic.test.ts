import assert from 'node:assert/strict';
import test from 'node:test';
import { openCrashReducerConnection } from '../src/stacks/spacetime-crash-transport.js';
import { executeAction } from '../src/actions/action-contract.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import type { CheckoutState } from '../src/stacks/checkout-state.js';
import { createCheckEvidence } from '../src/evidence/check-evidence.js';

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

test('crash action retains partial fault evidence and distinguishes recovered state from acknowledged loss', async () => {
  for (const mode of ['absent', 'committed', 'lost-acknowledged', 'partial', 'fault-error', 'cancelled-recovery', 'cancelled-read', 'recovery-error', 'disconnected-database', 'disconnected-application', 'drained-application', 'undrained-application', 'drained-recovery-error', 'disconnected-recovery-error', 'cancelled-disconnected-recovery-error']) {
    const cancellation = new AbortController();
    const timers: number[] = [];
    let recoveryStopped = false;
    let unsettled = false;
    const before: CheckoutState = { accountId: 'a', itemId: 'i', priceMinor: 100,
      stock: [{ warehouseId: 'w', quantity: 10 }], cart: [], reservations: [], orders: [], payments: [], orphanOrderLines: 0 };
    const prepared = structuredClone(before);
    prepared.cart.push({ itemId: 'i', quantity: 1 });
    const after = structuredClone(prepared);
    if (mode === 'committed' || mode === 'partial') {
      after.cart = []; after.stock[0]!.quantity--;
      after.orders.push({ id: 'o', accountId: 'a', status: 'pending', totalMinor: 100,
        lines: [{ itemId: 'i', quantity: 1, priceMinor: 100, allocations: [{ warehouseId: 'w', quantity: 1 }] }] });
      if (mode === 'committed') after.payments.push({ id: 'p', orderId: 'o', amountMinor: 100, status: 'paid' });
    }
    const wrap = (state: CheckoutState) => ({ state, schemaSha256: { schema: 'same' }, account: 'a', item: 'i', recordedAtMs: Date.now() });
    let release!: () => void;
    const waiting = new Promise<void>(resolve => { release = resolve; });
    const result = await executeAction(ACTION_REGISTRY, 'crashCheckout', {
      do: 'crashCheckout', actor: 'buyer', before: 'before', prepared: 'prepared', quantity: 1,
      requests: 1, offsetMs: 0, target: mode === 'disconnected-database' ? 'database' : 'application',
    }, { signal: cancellation.signal, onAbort: async () => {}, capabilities: {
      actors: { get: () => ({ name: 'buyer', writes: [{ headers: { authorization: 'Bearer private-token' } }] }) },
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
          return { ok: true, status: 200, text: async () => '' };
        } },
      'process-crash': { prepare: async () => ({ spacetime: null,
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
          return mode.endsWith('drained-application') ? { settled: mode === 'drained-application',
            startedAtMs: Date.now(), completedAtMs: Date.now(), backend: 'postgres', database: 'app',
            samples: [{ atMs: Date.now(), pending: mode === 'drained-application' ? 0 : 1 }] } : null;
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
    assert.equal(unsettled, mode.includes('disconnected-') || mode === 'undrained-application');
    assert.equal(result.status, mode.startsWith('cancelled-') || ['disconnected-database', 'disconnected-application', 'undrained-application'].includes(mode) ? 'inconclusive' : mode === 'fault-error' ? 'harness_failure'
      : ['partial', 'lost-acknowledged'].includes(mode) || mode.endsWith('recovery-error') ? 'failed' : 'passed', mode);
    if (mode === 'disconnected-recovery-error') assert.equal(result.code, 'application_failure');
    if (mode === 'drained-recovery-error') assert.equal((result.observation as { databaseDrain: { settled: boolean } }).databaseDrain.settled, true);
    assert(result.observation, mode);
    assert(!JSON.stringify(result.observation).includes('private-token'));
    assert.doesNotThrow(() => createCheckEvidence({ status: result.status, code: result.code, phase: 'assertion',
      observation: result.observation, actions: [{ actor: 'buyer', evidence: result }],
      startedAtMs: result.timing.startedAtMs, completedAtMs: result.timing.completedAtMs }));
  }
});
