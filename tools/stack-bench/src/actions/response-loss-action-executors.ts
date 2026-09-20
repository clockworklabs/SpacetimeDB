import { isDeepStrictEqual } from 'node:util';
import type { installResponseLoss } from '../../grader/response-loss.js';
import { actionImplementation, ActionApplicationFailure, ActionHarnessFailure, ActionInconclusive } from './action-contract.js';
import { actorFor } from './actor-action-runtime.js';
import type { ActorCapabilities } from './actor-action-runtime.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import { checkoutExpectation, type CheckoutQuantity, type createDatabaseReadCapability } from './runtime-action-executors.js';
import { orderCheckoutDifferences } from '../stacks/checkout-state.js';
import { evidenceNowMs } from '../evidence/evidence-timing.js';

interface Capabilities extends ActorCapabilities {
  'response-loss': {
    prepare(actor: string): Promise<void>;
    get(actor: string): Awaited<ReturnType<typeof installResponseLoss>>;
  };
  'database-read': ReturnType<typeof createDatabaseReadCapability>;
  clock: { sleep(ms: number, signal: AbortSignal): Promise<void> };
}

export const prepareResponseLoss = actionImplementation(async ({ input, capabilities }: {
  input: { actor: string }; capabilities: Pick<Capabilities, 'response-loss'>;
}) => {
  await capabilities['response-loss'].prepare(input.actor);
  return { actor: input.actor, installedBeforeReload: true };
});

// This establishes a committed operation with an undelivered reply. Recovery
// and same-cart replay use the existing actions and state assertions afterward.
export const loseCheckoutResponse = actionImplementation(async ({ input, capabilities, signal }: {
  input: { actor: string; before: string; prepared: string; quantity: CheckoutQuantity };
  capabilities: Capabilities; signal: AbortSignal;
}) => {
  const database = capabilities['database-read'];
  const before = database.checkoutSnapshots.get(input.before), prepared = database.checkoutSnapshots.get(input.prepared);
  if (!before || !prepared || before.scope !== 'orders' || prepared.scope !== 'orders'
    || !before.storage?.cart || before.account !== prepared.account || before.item !== prepared.item
    || !isDeepStrictEqual(before.schemaSha256, prepared.schemaSha256)) {
    throw new Error('response loss requires matching native before/cart snapshots');
  }
  const gate = capabilities['response-loss'].get(input.actor);
  let committed: ReturnType<typeof database.getCheckoutState> | undefined;
  let lastObserved: ReturnType<typeof database.getCheckoutState> | undefined;
  let failure: unknown;
  gate.arm();
  try {
    signal.throwIfAborted();
    await browserApplicationBoundary(() => actorFor(capabilities, input.actor).loc('checkout-submit').click())(undefined);
    const deadline = evidenceNowMs() + 10_000;
    do {
      signal.throwIfAborted();
      lastObserved = database.getCheckoutState(before);
      if (lastObserved.scope !== 'orders' || !isDeepStrictEqual(before.schemaSha256, lastObserved.schemaSha256)) {
        throw new Error('response-loss observer changed scope or schema');
      }
      const expected = checkoutExpectation(input.quantity, [before, prepared, lastObserved]);
      const differences = orderCheckoutDifferences(before.state, prepared.state, lastObserved.state,
        expected, false, before.storage.warehouses);
      const evidence = gate.evidence();
      if (evidence.errors.length || evidence.truncated) break;
      const events = evidence.events;
      const dropped = events.some(e => e.kind === 'http-response')
        || events.some(e => e.kind === 'ws-send') && events.some(e => e.kind === 'ws-drop');
      if (!differences.length && dropped) { committed = lastObserved; break; }
      await capabilities.clock.sleep(50, signal);
    } while (evidenceNowMs() < deadline);
  } catch (error) { failure = error; }
  finally { await gate.finish(); }
  const fault = gate.evidence();
  const observation = { before: input.before, prepared: input.prepared, fault,
    ...(lastObserved ? { after: lastObserved } : {}), committed: Boolean(committed) };
  if (failure) {
    if (signal.aborted) throw failure;
    if (failure instanceof ActionApplicationFailure) throw new ActionApplicationFailure(failure.message, { ...failure.details, observation });
    if (failure instanceof ActionInconclusive) throw new ActionInconclusive(failure.message, { ...failure.details, observation });
    throw new ActionHarnessFailure(failure instanceof Error ? failure.message : String(failure), { observation });
  }
  if (fault.errors.length || fault.truncated) {
    throw new ActionHarnessFailure('response-loss gate has incomplete evidence', { observation });
  }
  if (!committed) throw new ActionInconclusive('could not establish both checkout commit and suppressed reply', { observation });
  return observation;
});
