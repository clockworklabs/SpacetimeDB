import type { Page } from 'playwright';
import { actionImplementation } from './action-contract.js';
import { actorFor, browserFor, type ActorActionArguments, type BrowserActorCapabilities } from './actor-action-runtime.js';
import { withAuthWriteInventory, withAuthWriteTarget } from './auth-request-patch.js';
import { dispatchNested, type ConcurrencyCapability } from './runtime-action-executors.js';

interface Step extends Record<string, unknown> { do: string }
interface SignupClaimsInput {
  actor: string;
  name: string;
  fields: Record<string, unknown>;
  branches: readonly (readonly Step[])[];
}

export const probeSignupClaims = actionImplementation(async ({ input, capabilities, signal }:
  ActorActionArguments<SignupClaimsInput, BrowserActorCapabilities & { concurrency: ConcurrencyCapability }>) => {
  const dispatch = (step: Step) => dispatchNested(capabilities.concurrency, step, signal);
  const page = actorFor(capabilities, input.actor).page as Page;
  const readEndpoints = browserFor(capabilities).authReadEndpoints ?? [];
  const ordinary = `${input.name}-ordinary`;
  const baseline = await withAuthWriteInventory(page, async () => {
    await dispatch({ do: 'signUp', actor: input.actor, name: ordinary, deferAuthFailureToExpect: true });
    await dispatch({ do: 'expect', actor: input.actor, testid: 'current-user',
      contains: `{user:${ordinary}}`, within: 10000 });
  }, readEndpoints);
  // Keep the ordinary-account sign-in control outside the registration inventory.
  await dispatch({ do: 'click', actor: input.actor, testid: 'current-user', unlessVisible: 'signout' });
  await dispatch({ do: 'click', actor: input.actor, testid: 'signout' });
  await dispatch({ do: 'waitUntilAbsent', actor: input.actor, testid: 'current-user', within: 6000 });
  await dispatch({ do: 'signIn', actor: input.actor, name: ordinary });
  await dispatch({ do: 'expect', actor: input.actor, testid: 'current-user',
    contains: `{user:${ordinary}}`, within: 10000 });

  for (const step of input.branches[0]!) await dispatch(step);
  let previousActor = input.actor;
  const probes: unknown[] = [];
  for (let index = 0; index < baseline.writes.length; index++) {
    const fresh = await dispatch({ do: 'freshClient', actor: previousActor, preserveStorage: false }) as { actor: string };
    previousActor = fresh.actor;
    try {
      const targetPage = actorFor(capabilities, fresh.actor).page as Page;
      const result = await withAuthWriteTarget(targetPage, { writes: baseline.writes, index, readEndpoints }, () => dispatch({
        do: 'signUp', actor: fresh.actor, name: `${input.name}-${index}`, requestPatch: { fields: input.fields },
      }));
      for (const step of input.branches[0]!) {
        await dispatch({ ...step, ...(step.actor === input.actor ? { actor: fresh.actor } : {}) });
      }
      probes.push({ index, write: baseline.writes[index], result });
    } finally {
      await dispatch({ do: 'closeClient', actor: fresh.actor });
    }
  }
  return { writes: baseline.writes, probes };
});
