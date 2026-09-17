import { isDeepStrictEqual } from 'node:util';
import { actionImplementation, ActionApplicationFailure, ActionHarnessFailure, ActionInconclusive } from './action-contract.js';
import { actorFor, inconclusive } from './actor-action-runtime.js';
import type { ActorCapabilities } from './actor-action-runtime.js';
import { browserCredentials, capturedCredentials, namedActionRequest } from './named-action-runtime.js';
import type { NamedAction, NamedActionsCapability } from './named-action-runtime.js';
import type { createDatabaseReadCapability } from './runtime-action-executors.js';
import { checkoutDifferences, orderCheckoutDifferences } from '../stacks/checkout-state.js';
import { openCrashReducerConnection } from '../stacks/spacetime-crash-transport.js';
import type { CrashTarget, ProcessCrashReceipt } from '../stacks/process-crash.js';
import type { DatabaseDrainReceipt, PreparedRuntimeCrash } from '../runtime/backend-control.js';
import { finding, renderFinding } from './action-findings.js';
import type { SpacetimeTarget } from '../stacks/stack-grading-operations.js';

interface Input {
  actor: string; before: string; prepared: string; quantity: number;
  requests: 1 | 16; offsetMs: 0 | 5 | 20; target: CrashTarget;
  namedAction?: NamedAction;
}
interface Capabilities extends ActorCapabilities {
  'named-actions': NamedActionsCapability;
  'database-read': ReturnType<typeof createDatabaseReadCapability>;
  'process-crash': { prepare(target: CrashTarget): Promise<PreparedRuntimeCrash> };
}

// Setup and fault calls must use the same completion semantics.
async function checkoutCaller(input: { actor: string; namedAction?: NamedAction },
  capabilities: Pick<Capabilities, 'actors' | 'named-actions'>, signal: AbortSignal,
  spacetime: SpacetimeTarget | null | undefined) {
  const named = capabilities['named-actions'];
  const actor = actorFor(capabilities, input.actor);
  const credentials = capturedCredentials(actor) ?? await browserCredentials(actor);
  if (!credentials) inconclusive('no-session', { actor: input.actor, action: 'checkout' });
  const action = input.namedAction ?? named.resolve('checkout');
  if (!action) inconclusive('unknown-action', { action: 'checkout' });
  const request = namedActionRequest(named, action, {});
  if (!request?.url) inconclusive('unresolved-action', { action: 'checkout' });
  let connection: Awaited<ReturnType<typeof openCrashReducerConnection>> | undefined;
  if (spacetime) {
    const authorization = Object.entries(credentials).find(([key]) => key.toLowerCase() === 'authorization')?.[1];
    const token = authorization?.match(/^Bearer (\S+)$/i)?.[1];
    if (!token || !action.reducer) inconclusive('no-session', { actor: input.actor, action: 'checkout' });
    connection = await openCrashReducerConnection(spacetime, token, signal);
  }
  return {
    close: () => connection?.close(),
    async call() {
      const startedAtMs = named.now();
      const timeout = AbortSignal.timeout(30_000), requestSignal = AbortSignal.any([signal, timeout]);
      let outcome: 'committed' | 'not-confirmed' = 'not-confirmed', status: number | null = null;
      let responseFailed = false;
      try {
        if (connection) {
          const reply = await connection.call(action.reducer!, request.body ?? '[]', requestSignal);
          if (reply.outcome === 'committed') outcome = 'committed';
          responseFailed = reply.outcome === 'refused';
        } else {
          const reply = await named.fetch(request.url!, { method: request.method ?? 'POST',
            headers: { 'Content-Type': 'application/json', ...credentials }, body: request.body, signal: requestSignal });
          await reply.text();
          status = reply.status;
          // 202 acknowledges queued work, not a completed checkout.
          if (reply.ok && reply.status !== 202) outcome = 'committed';
          responseFailed = !reply.ok;
        }
      } catch { /* A disconnected or failed response does not prove rollback. */ }
      return { actor: input.actor, action: 'checkout', startedAtMs, completedAtMs: named.now(), outcome, status, responseFailed,
        cancelled: signal.aborted, timedOut: timeout.aborted,
        protocol: connection ? 'websocket-v1-confirmed' : 'http' };
    },
  };
}

export const confirmCheckout = actionImplementation(async ({ input, capabilities, signal }: {
  input: { actor: string; namedAction?: NamedAction };
  capabilities: Pick<Capabilities, 'actors' | 'named-actions' | 'database-read'>; signal: AbortSignal;
}) => {
  const caller = await checkoutCaller(input, capabilities, signal, capabilities['named-actions'].spacetime);
  try {
    const receipt = await caller.call();
    if (receipt.outcome === 'committed') return receipt;
    // A correlated native refusal is complete. An HTTP error can follow a commit.
    if (receipt.protocol === 'http' || !receipt.responseFailed) capabilities['database-read'].markCheckoutUnsettled();
    if (receipt.responseFailed) {
      const value = finding('number-mismatch', { control: 'completed checkout', observed: 0, expected: { equals: 1 } });
      throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation: receipt });
    }
    const value = finding('invalid-input', { detail: 'baseline checkout has no verified completion receipt' });
    throw new ActionInconclusive(renderFinding(value), { finding: value, observation: receipt });
  } finally { caller.close(); }
});

export const crashCheckout = actionImplementation(async ({ input, capabilities, signal }: {
  input: Input; capabilities: Capabilities; signal: AbortSignal;
}) => {
  const named = capabilities['named-actions'], database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before), prepared = database.checkoutSnapshots.get(input.prepared);
  if (!before || !prepared || !prepared.recordedAtMs) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
  if (before.storage && !before.storage.cart) {
    throw new Error('checkout crash reconciliation requires cart evidence');
  }
  if (!isDeepStrictEqual(before.schemaSha256, prepared.schemaSha256)
    || !isDeepStrictEqual(before.storage, prepared.storage)
    || before.scope !== prepared.scope
    || before.account !== prepared.account || before.item !== prepared.item) throw new Error('crash snapshots do not describe the same verified state');
  const compare = (after: typeof before.state, allowUnchanged: boolean) => before.scope === 'orders'
    ? orderCheckoutDifferences(before.state, prepared.state, after, input.quantity, allowUnchanged, before.storage?.warehouses ?? true)
    : checkoutDifferences(before.state, prepared.state, after, input.quantity, allowUnchanged);
  const setupDifferences = compare(prepared.state, true);
  if (setupDifferences.length) inconclusive('invalid-input', { detail: 'checkout crash requires a valid prepared cart' });
  const runtime = await capabilities['process-crash'].prepare(input.target);
  let caller: Awaited<ReturnType<typeof checkoutCaller>> | undefined;
  let unsettled = false;
  try {
    caller = await checkoutCaller(input, capabilities, signal, runtime.spacetime);
    // Reference reservations last 90 seconds. Leave time for recovery and mark
    // expired trials unmeasured; expiry is not a failed atomic checkout.
    if (prepared.state.reservations.length && Date.now() - prepared.recordedAtMs > 30_000) {
      inconclusive('invalid-input', { detail: 'prepared cart is too old for a bounded crash trial' });
    }
    let receipt: ProcessCrashReceipt | undefined, faultError: unknown, recoveryError: unknown, recoveredAtMs: number | undefined;
    let databaseDrain: DatabaseDrainReceipt | null | undefined;
    const pending = Array.from({ length: input.requests }, async (_, index) => ({
      requestIndex: index + 1, ...await caller!.call(),
    }));
    const fault = (async () => {
      signal.throwIfAborted();
      if (input.offsetMs) await named.sleep(input.offsetMs, signal);
      try { receipt = await runtime.crash(); }
      catch (error) {
        faultError = error;
        if (error && typeof error === 'object' && 'receipt' in error) receipt = error.receipt as ProcessCrashReceipt;
      }
      // Recover while requests are still pending, so ordinary driver recovery can
      // finish them. The harness does not reset data. Application startup and
      // reader connection hooks can reconstruct it; compare recovered state.
      try {
        signal.throwIfAborted();
        databaseDrain = await runtime.recover(AbortSignal.any([signal,
          AbortSignal.timeout(input.target === 'application' ? 80_000 : 45_000)]));
        recoveredAtMs = named.now();
      }
      catch (error) {
        recoveryError = error;
        if (error && typeof error === 'object' && 'databaseDrain' in error) {
          databaseDrain = error.databaseDrain as DatabaseDrainReceipt | null;
        }
      }
    })().catch(error => { faultError ??= error; });
    const outcomes = await Promise.all(pending);
    const queued = outcomes.some(row => row.status === 202);
    // Killing the app cannot retract a commit already sent to its database.
    unsettled = !runtime.spacetime && outcomes.some(row => row.status === null);
    await fault;
    if (databaseDrain) unsettled = !databaseDrain.settled;
    // An idle database cannot prove that an application queue is empty.
    unsettled ||= queued;
    const observation = { before, prepared, outcomes, unsettled, databaseDrain: databaseDrain ?? null,
      receipt: receipt ?? null, recoveredAtMs: recoveredAtMs ?? null,
      faultError: faultError instanceof Error ? faultError.message : faultError ? String(faultError) : null,
      recoveryError: recoveryError instanceof Error ? recoveryError.message : recoveryError ? String(recoveryError) : null };
    if (faultError || !receipt || signal.aborted) {
      throw new ActionHarnessFailure('crash trial did not complete fault and recovery', { observation });
    }
    const appRecoveryFailed = recoveryError && typeof recoveryError === 'object' && 'code' in recoveryError
      && recoveryError.code === 'generated_app_not_restartable';
    if (recoveryError && !appRecoveryFailed) {
      throw new ActionHarnessFailure('database recovery was not verified', { observation });
    }
    const deadline = Date.now() + 20_000;
    let after: ReturnType<typeof database.getCheckoutState>;
    while (true) {
      try { after = database.getCheckoutState(prepared); break; }
      catch (error) {
        if (Date.now() >= deadline || signal.aborted) throw new ActionHarnessFailure('stored state unavailable after recovery', {
          observation: { ...observation, readError: error instanceof Error ? error.message : String(error) },
        });
        try { await named.sleep(250, signal); }
        catch { throw new ActionHarnessFailure('stored state read interrupted after recovery', { observation }); }
      }
    }
    if (!isDeepStrictEqual(after.schemaSha256, prepared.schemaSha256) || after.scope !== prepared.scope
      || !isDeepStrictEqual(after.storage, prepared.storage)) {
      throw new ActionHarnessFailure('crash reader schema changed', { observation: { ...observation, after } });
    }
    const confirmed = outcomes.some(row => row.outcome === 'committed');
    const differences = compare(after.state, !confirmed);
    const signalTimes = [...receipt.processEvidence.matchAll(/^KILLED \d+ \d+ (\d+)$/gm)].map(match => Number(match[1]) - receipt!.clockOffsetBeforeMs);
    const faultAtMs = Math.min(...signalTimes);
    const faultEndMs = Math.max(...signalTimes);
    const outstandingAtFault = outcomes.filter(row => row.startedAtMs <= faultAtMs && row.completedAtMs >= faultEndMs).length;
    const evidence = { ...observation, after, observedAtMs: named.now(), differences, confirmed, faultAtMs, faultEndMs, outstandingAtFault };
    const unmeasured = prepared.state.reservations.length && Date.now() - prepared.recordedAtMs >= 85_000 ? 'reservation expiry prevents a complete recovery comparison'
      : Math.abs(receipt.clockOffsetAfterMs - receipt.clockOffsetBeforeMs) > 5 ? 'clock changed during fault'
        : !outstandingAtFault ? 'fault missed the outstanding-request window'
          : unsettled && !appRecoveryFailed ? queued ? 'asynchronous checkout has no verified completion receipt'
            : 'a disconnected checkout may still be running in the database' : null;
    if (unmeasured) {
      const value = finding('invalid-input', { detail: unmeasured });
      throw new ActionInconclusive(renderFinding(value), { finding: value, observation: evidence });
    }
    if (appRecoveryFailed) {
      const value = finding('app-control-failed', { mode: 'start', target: 'app-server', detail: observation.recoveryError! });
      throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation: evidence });
    }
    if (differences[0]) {
      const { control, observed, expected } = differences[0];
      const value = finding('number-mismatch', { control, observed, expected: { equals: expected } });
      throw new ActionApplicationFailure(renderFinding(value), { finding: value, observation: evidence });
    }
    return evidence;
  } finally {
    if (unsettled) database.markCheckoutUnsettled();
    caller?.close();
    await runtime.close();
  }
});
