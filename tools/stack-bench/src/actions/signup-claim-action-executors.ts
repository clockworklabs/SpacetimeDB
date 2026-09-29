import type { Page } from 'playwright';
import { actionImplementation } from './action-contract.js';
import { actorFor, browserFor, type ActorActionArguments, type BrowserActorCapabilities } from './actor-action-runtime.js';
import { withAuthSubmitCapture, withAuthWriteInventory, withAuthWriteTarget, type AuthWrite } from './auth-request-patch.js';
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
  let writes: readonly AuthWrite[] = [];
  await withAuthSubmitCapture(page,
    () => dispatch({ do: 'signUp', actor: input.actor, name: ordinary, deferAuthFailureToExpect: true }),
    async submit => {
      const baseline = await withAuthWriteInventory(page, async () => {
        const result = await submit();
        await dispatch({ do: 'expect', actor: input.actor, testid: 'current-user',
          contains: `{user:${ordinary}}`, within: 10000 });
        return result;
      }, readEndpoints);
      writes = baseline.writes;
      return baseline.result;
    });
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
  for (let index = 0; index < writes.length; index++) {
    const fresh = await dispatch({ do: 'freshClient', actor: previousActor, preserveStorage: false }) as { actor: string };
    previousActor = fresh.actor;
    try {
      const targetPage = actorFor(capabilities, fresh.actor).page as Page;
      const claimant = `${input.name}-${index}`;
      const result = await withAuthWriteTarget(targetPage, { writes, index, readEndpoints }, () => dispatch({
        do: 'signUp', actor: fresh.actor, name: claimant, requestPatch: { fields: input.fields },
      }));
      // An app may create the claimed account but leave it signed out (or show an
      // error). Sign in once so the branch cannot pass on an anonymous request; if
      // no account exists, the anonymous request is the true state of the claim.
      const currentUser = actorFor(capabilities, fresh.actor).loc('current-user');
      const signedIn = async () => await currentUser.isVisible()
        && (await currentUser.innerText()).includes(browserFor(capabilities).scopedUser(claimant));
      if (!(await signedIn())) {
        await dispatch({ do: 'signIn', actor: fresh.actor, name: claimant, expectFailure: true });
      }
      for (const step of input.branches[0]!) {
        await dispatch({ ...step, ...(step.actor === input.actor ? { actor: fresh.actor } : {}) });
      }
      probes.push({ index, write: writes[index], result });
    } finally {
      await dispatch({ do: 'closeClient', actor: fresh.actor });
    }
  }
  return { writes, probes };
});
