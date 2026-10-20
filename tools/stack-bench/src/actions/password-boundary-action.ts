import { actionImplementation } from './action-contract.js';
import { actorFor, browserFor, type ActorActionArguments, type BrowserActorCapabilities } from './actor-action-runtime.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import { dispatchNested, type ConcurrencyCapability } from './runtime-action-executors.js';

interface PasswordBoundaryInput {
  actor: string;
  returningActor: string;
  impostorActor: string;
  name: string;
  password: string;
  wrongPassword: string;
}

// This input exceeds the common password support range. Refusing it is safe;
// accepting it requires every supplied byte to participate in authentication.
export const probePasswordBoundary = actionImplementation(browserApplicationBoundary(async ({ input, capabilities, signal }:
  ActorActionArguments<PasswordBoundaryInput, BrowserActorCapabilities & { concurrency: ConcurrencyCapability }>) => {
  const dispatch = (step: Record<string, unknown> & { do: string }) =>
    dispatchNested(capabilities.concurrency, step, signal);
  const owner = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  await dispatch({ do: 'signUp', actor: input.actor, name: input.name,
    password: input.password, awaitSignedIn: false });
  // Neither a timeout nor a silent form reset proves a safe refusal.
  await owner.loc('current-user').or(owner.loc('auth-error')).filter({ visible: true }).first()
    .waitFor({ state: 'visible', timeout: browser.defaultWithin * 2 });
  const refused = await owner.loc('auth-error').isVisible();
  await dispatch({ do: 'expect', actor: input.actor, testid: 'current-user',
    ...(refused ? { absent: true } : { contains: `{user:${input.name}}` }), within: 6000 });
  if (refused) {
    await dispatch({ do: 'reload', actor: input.actor, application: true, settleMs: 1000 });
    await dispatch({ do: 'expect', actor: input.actor, testid: 'current-user', absent: true });
  }
  await dispatch({ do: 'signIn', actor: input.returningActor, name: input.name,
    password: input.password, ...(refused ? { expectFailure: true } : { awaitSignedIn: false }) });
  await dispatch({ do: 'expect', actor: input.returningActor,
    testid: refused ? 'auth-error' : 'current-user',
    ...(refused ? {} : { contains: `{user:${input.name}}` }), within: 10000 });
  if (refused) {
    await dispatch({ do: 'reload', actor: input.returningActor, application: true, settleMs: 1000 });
    await dispatch({ do: 'expect', actor: input.returningActor, testid: 'current-user', absent: true });
  }
  await dispatch({ do: 'signIn', actor: input.impostorActor, name: input.name,
    password: input.wrongPassword, expectFailure: true });
  await dispatch({ do: 'expect', actor: input.impostorActor, testid: 'auth-error', within: 6000 });
  await dispatch({ do: 'reload', actor: input.impostorActor, application: true, settleMs: 1000 });
  await dispatch({ do: 'expect', actor: input.impostorActor, testid: 'current-user', absent: true });
  return { disposition: refused ? 'unsupported-password-refused' : 'complete-password-verified' };
}));
