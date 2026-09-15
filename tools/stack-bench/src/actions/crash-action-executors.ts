import { isDeepStrictEqual } from 'node:util';
import { actionImplementation, ActionApplicationFailure, ActionHarnessFailure, ActionInconclusive } from './action-contract.js';
import { actorFor, inconclusive } from './actor-action-runtime.js';
import type { ActorCapabilities } from './actor-action-runtime.js';
import { browserCredentials, capturedCredentials, namedActionRequest } from './named-action-runtime.js';
import type { NamedAction, NamedActionsCapability } from './named-action-runtime.js';
import type { createDatabaseReadCapability } from './runtime-action-executors.js';
import { checkoutDifferences } from '../stacks/checkout-state.js';
import { openCrashReducerConnection } from '../stacks/spacetime-crash-transport.js';
import type { CrashTarget, ProcessCrashReceipt } from '../stacks/process-crash.js';
import type { PreparedRuntimeCrash } from '../runtime/backend-control.js';
import { finding, renderFinding } from './action-findings.js';

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

export const crashCheckout = actionImplementation(async ({ input, capabilities, signal }: {
  input: Input; capabilities: Capabilities; signal: AbortSignal;
}) => {
  const named = capabilities['named-actions'], database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before), prepared = database.checkoutSnapshots.get(input.prepared);
  if (!before || !prepared || !prepared.recordedAtMs) inconclusive('assertion-without-action', { action: 'dbRecordCheckout' });
  if (!isDeepStrictEqual(before.schemaSha256, prepared.schemaSha256)
    || before.account !== prepared.account || before.item !== prepared.item) throw new Error('crash snapshots do not describe the same verified state');
  const setupDifferences = checkoutDifferences(before.state, prepared.state, prepared.state, input.quantity, true);
  if (setupDifferences.length) inconclusive('invalid-input', { detail: 'checkout crash requires a valid prepared cart' });
  const actor = actorFor(capabilities, input.actor);
  const credentials = capturedCredentials(actor) ?? await browserCredentials(actor);
  if (!credentials) inconclusive('no-session', { actor: input.actor, action: 'checkout' });
  const action = input.namedAction ?? named.resolve('checkout');
  if (!action) inconclusive('unknown-action', { action: 'checkout' });
  const request = namedActionRequest(named, action, {});
  if (!request?.url) inconclusive('unresolved-action', { action: 'checkout' });
  const runtime = await capabilities['process-crash'].prepare(input.target);
  let connection: Awaited<ReturnType<typeof openCrashReducerConnection>> | undefined;
  let unsettled = false;
  try {
    if (runtime.spacetime) {
      const authorization = Object.entries(credentials).find(([key]) => key.toLowerCase() === 'authorization')?.[1];
      const token = authorization?.match(/^Bearer (\S+)$/i)?.[1];
      if (!token || !action.reducer) inconclusive('no-session', { actor: input.actor, action: 'checkout' });
      connection = await openCrashReducerConnection(runtime.spacetime, token, signal);
    }
    // Reference reservations last 90 seconds. Leave time for recovery and mark
    // expired trials unmeasured; expiry is not a failed atomic checkout.
    if (Date.now() - prepared.recordedAtMs > 30_000) {
      inconclusive('invalid-input', { detail: 'prepared cart is too old for a bounded crash trial' });
    }
    let receipt: ProcessCrashReceipt | undefined, faultError: unknown, recoveryError: unknown, recoveredAtMs: number | undefined;
    const pending = Array.from({ length: input.requests }, (_, index) => (async () => {
      const startedAtMs = named.now();
      const timeout = AbortSignal.timeout(30_000), requestSignal = AbortSignal.any([signal, timeout]);
      let outcome: 'committed' | 'not-confirmed' = 'not-confirmed', status: number | null = null;
      try {
        if (connection) {
          const reply = await connection.call(action.reducer!, request.body ?? '[]', requestSignal);
          if (reply.outcome === 'committed') outcome = 'committed';
        } else {
          const reply = await named.fetch(request.url!, { method: request.method ?? 'POST',
            headers: { 'Content-Type': 'application/json', ...credentials }, body: request.body, signal: requestSignal });
          await reply.text();
          status = reply.status;
          if (reply.ok) outcome = 'committed';
        }
      } catch { /* A disconnected or failed response does not prove rollback. */ }
      return { requestIndex: index + 1, actor: input.actor, startedAtMs, completedAtMs: named.now(),
        outcome, status, cancelled: signal.aborted, timedOut: timeout.aborted,
        protocol: connection ? 'websocket-v1-confirmed' : 'http' };
    })());
    const fault = (async () => {
      signal.throwIfAborted();
      if (input.offsetMs) await named.sleep(input.offsetMs, signal);
      try { receipt = await runtime.crash(); }
      catch (error) {
        faultError = error;
        if (error && typeof error === 'object' && 'receipt' in error) receipt = error.receipt as ProcessCrashReceipt;
      }
      // Recover while requests are still pending, so ordinary driver recovery can
      // finish them. Recovery never resets or reseeds stored data.
      try {
        signal.throwIfAborted();
        await runtime.recover(AbortSignal.any([signal, AbortSignal.timeout(45_000)]));
        recoveredAtMs = named.now();
      }
      catch (error) { recoveryError = error; }
    })().catch(error => { faultError ??= error; });
    const outcomes = await Promise.all(pending);
    unsettled = input.target === 'database' && !runtime.spacetime && outcomes.some(row => row.status === null);
    await fault;
    const observation = { before, prepared, outcomes, unsettled, receipt: receipt ?? null, recoveredAtMs: recoveredAtMs ?? null,
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
    if (!isDeepStrictEqual(after.schemaSha256, prepared.schemaSha256)) {
      throw new ActionHarnessFailure('crash reader schema changed', { observation: { ...observation, after } });
    }
    const confirmed = outcomes.some(row => row.outcome === 'committed');
    const differences = checkoutDifferences(before.state, prepared.state, after.state, input.quantity, !confirmed);
    const signalTimes = [...receipt.processEvidence.matchAll(/^KILLED \d+ \d+ (\d+)$/gm)].map(match => Number(match[1]) - receipt!.clockOffsetBeforeMs);
    const faultAtMs = Math.min(...signalTimes);
    const faultEndMs = Math.max(...signalTimes);
    const outstandingAtFault = outcomes.filter(row => row.startedAtMs <= faultAtMs && row.completedAtMs >= faultEndMs).length;
    const evidence = { ...observation, after, observedAtMs: named.now(), differences, confirmed, faultAtMs, faultEndMs, outstandingAtFault };
    const unmeasured = unsettled ? 'a disconnected checkout may still be running after database recovery'
      : Date.now() - prepared.recordedAtMs >= 85_000 ? 'reservation expiry prevents a complete recovery comparison'
      : Math.abs(receipt.clockOffsetAfterMs - receipt.clockOffsetBeforeMs) > 5 ? 'clock changed during fault'
        : !outstandingAtFault ? 'fault missed the outstanding-request window' : null;
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
    connection?.close();
    await runtime.close();
  }
});
