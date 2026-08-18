import { execFileSync } from 'node:child_process';

import { ActionApplicationFailure, ActionInconclusive } from './action-contract.mjs';
import { evidenceDisposition } from '../evidence/check-evidence.mjs';
import { replayHeaders } from './actor-transport-action-executors.mjs';
import { browserApplicationBoundary } from './browser-action-executors.mjs';
import { controlBackend } from '../runtime/backend-control.mjs';
import { harnessBrowserFailure, harnessProcessFailure } from '../evidence/harness-errors.mjs';
import { executeStackCapability, StackCapabilityUnsupportedError } from '../stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.mjs';

export const LIFECYCLE_CONCURRENCY_ACTION_IDS = Object.freeze([
  'clickConcurrently',
  'closeClient',
  'dbSetStock',
  'freshClient',
  'openClient',
  'race',
  'replayConcurrently',
  'restartBackend',
  'sendConcurrently',
  'setOffline',
  'startAppServer',
  'stopAppServer',
].sort());

const fail = message => { throw new ActionApplicationFailure(message); };
const inconclusive = message => { throw new ActionInconclusive(message); };

async function dispatchNested(concurrency, step, signal) {
  try {
    return await concurrency.dispatch(step, signal);
  } catch (error) {
    const evidence = error?.actionEvidence;
    const disposition = evidence ? evidenceDisposition(evidence) : null;
    if (disposition?.applicationFailure) fail(evidence.summary ?? `${step.do} failed`);
    if (disposition?.outcomeKind === 'inconclusive') {
      inconclusive(evidence.summary ?? `${step.do} was inconclusive`);
    }
    throw error;
  }
}

function actorFor(capabilities, name) {
  const actor = capabilities.actors.get(name);
  if (!actor) fail(`unknown actor "${name}"`);
  return actor;
}

async function replayConcurrently({ input, capabilities, signal }) {
  const concurrency = capabilities.concurrency;
  const method = input.method ?? 'POST';
  const pick = actor => {
    if (input.match) {
      return [...(actor.writes ?? [])].reverse()
        .find(write => write.method === method && write.url.includes(input.match));
    }
    return actor.lastWrites?.[method] ?? actor.lastWrite;
  };
  const pending = input.actors.map(name => {
    const actor = actorFor(capabilities, name);
    return { actor, write: pick(actor) };
  }).filter(candidate => candidate.write);
  if (pending.length < 2) {
    inconclusive(`fewer than two ${input.match ? `"${input.match}" ` : ''}write requests were captured, `
      + 'so nothing was contended. This backend may not write over HTTP, or the request carried no JSON body '
      + '(only bodied writes reach lastWrites — pass `match` to select by URL instead).');
  }
  const replies = await Promise.all(pending.map(({ actor, write }) =>
    actor.page.request.fetch(write.url, {
      method: write.method,
      headers: replayHeaders(write),
      data: write.body === undefined || write.body === null ? undefined : JSON.stringify(write.body),
    }).then(response => response.status(), error => `error: ${String(error.message).split('\n')[0]}`)));
  const answered = replies.filter(status => typeof status === 'number');
  if (answered.length < 2) {
    inconclusive(`only ${answered.length} of ${pending.length} replayed ${input.match ?? 'write'} requests `
      + `reached the server (responses: ${replies.join(', ')}), so the two never contended.`);
  }
  await concurrency.sleep(input.settleMs ?? 3000, signal);
  return { attempted: pending.length, answered: answered.length, replies };
}

async function clickConcurrently({ input, capabilities, signal }) {
  const concurrency = capabilities.concurrency;
  const targets = input.targets ?? input.actors.map(actor => ({ actor }));
  const resolved = targets.map(target => {
    const where = target.in ?? input.in;
    const scope = where
      ? { testid: where.testid, contains: concurrency.expand(where.contains) }
      : undefined;
    return { target, locator: actorFor(capabilities, target.actor).loc(input.testid, { scope }) };
  });
  const notReady = (await Promise.all(resolved.map(async ({ target, locator }) => {
    try {
      await locator.waitFor({ state: 'visible', timeout: input.readyWithin ?? 15000 });
      return await locator.isEnabled() ? null : target.actor;
    } catch (error) {
      if (harnessBrowserFailure(error)) throw error;
      return target.actor;
    }
  }))).filter(Boolean);
  if (notReady.length) {
    inconclusive(`${concurrency.testId(input.testid)} never became clickable for ${notReady.join(', ')} `
      + '— the page was not ready, so nothing could be contended');
  }
  const outcomes = await Promise.all(resolved.map(({ target, locator }) =>
    locator.click({ timeout: input.within ?? concurrency.defaultWithin, force: true, noWaitAfter: true })
      .then(() => null, error => `${target.actor}: ${String(error.message).split('\n')[0]}`)));
  const failed = outcomes.filter(Boolean);
  if (failed.length) {
    inconclusive(`${failed.length} of ${targets.length} concurrent clicks on `
      + `${concurrency.testId(input.testid)} never dispatched, so nothing was actually contended — `
      + failed.join(' | '));
  }
  await concurrency.sleep(input.settleMs ?? 3000, signal);
  return { dispatched: targets.length };
}

async function race({ input, capabilities, signal }) {
  const concurrency = capabilities.concurrency;
  await Promise.all(input.branches.map(async branch => {
    for (const step of branch) await dispatchNested(concurrency, step, signal);
  }));
  await concurrency.sleep(input.settleMs ?? 2000, signal);
  return { branches: input.branches.length };
}

async function sendConcurrently({ input, capabilities, signal }) {
  const concurrency = capabilities.concurrency;
  await Promise.all(input.senders.map(sender => dispatchNested(concurrency, {
    do: 'sendMany',
    actor: sender.actor,
    prefix: sender.prefix,
    count: sender.count,
    delayMs: sender.delayMs ?? input.delayMs ?? 0,
  }, signal)));
  return { senders: input.senders.length,
    messages: input.senders.reduce((total, sender) => total + sender.count, 0) };
}

async function restartBackend({ input, capabilities, signal }) {
  await capabilities['backend-lifecycle'].operate('restart', input.settleMs ?? 10000, signal);
  return { operation: 'restart' };
}

async function startAppServer({ input, capabilities, signal }) {
  await capabilities['application-lifecycle'].operate('start', input.settleMs ?? 8000, signal);
  return { operation: 'start' };
}

async function stopAppServer({ input, capabilities, signal }) {
  await capabilities['application-lifecycle'].operate('stop', input.settleMs ?? 2000, signal);
  return { operation: 'stop' };
}

async function dbSetStock({ input, capabilities, signal }) {
  let result;
  try {
    result = await capabilities['database-write'].setStock(input);
  } catch (error) {
    if (error?.classification || harnessProcessFailure(error)) throw error;
    fail(`dbSetStock failed: ${((error.stdout ?? '') + (error.stderr ?? '')).trim().slice(-200)
      || error.message}`);
  }
  await capabilities.clock.sleep(input.settleMs, signal);
  return result;
}

async function setOffline({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  const browser = capabilities['browser-interaction'];
  const offline = input.offline !== false;
  await actor.page.context().setOffline(offline);
  await browser.sleep(input.settleMs ?? 500, signal);
  const browserOnline = await actor.page.evaluate(() => navigator.onLine);
  if (browserOnline === offline) {
    throw new Error(`setOffline requested browser network ${offline ? 'offline' : 'online'}, `
      + `but navigator.onLine remained ${browserOnline}`);
  }
  return { offline, browserOnline };
}

async function closeClient({ input, capabilities }) {
  await actorFor(capabilities, input.actor).page.close();
  return { closed: true };
}

async function openClient({ input, capabilities, signal }) {
  const actor = actorFor(capabilities, input.actor);
  await capabilities['browser-interaction'].clients.open(actor, input.settleMs ?? 4000, signal);
  return { opened: true };
}

async function freshClient({ input, capabilities }) {
  const actor = actorFor(capabilities, input.actor);
  const name = await capabilities['browser-interaction'].clients.fresh(actor, input.actor);
  return { actor: name };
}

export function createLifecycleCapability({ restartSpec, restartCmd, application = false,
  sleep, control = controlBackend, exec = execFileSync }) {
  return Object.freeze({
    async operate(mode, settleMs, signal) {
      if (!restartSpec && !restartCmd) {
        inconclusive(application
          ? 'no backend control supplied, cannot control the app server'
          : 'no backend control supplied, backend was never restarted');
      }
      try {
        if (restartSpec) await control(restartSpec, mode, { signal });
        else {
          const command = application ? `${restartCmd} ${mode}` : restartCmd;
          exec('bash', ['-c', command], { stdio: 'ignore', timeout: 300000 });
        }
      } catch (error) {
        if (error.status === 3) {
          inconclusive(application
            ? 'app-server control refused on this host'
            : 'backend restart refused — no benchmark-owned instance available');
        }
        if (harnessProcessFailure(error)) throw error;
        fail(application
          ? `could not ${mode} the app server: ${(error.stdout || error.message || '').toString().trim().slice(-160)}`
          : `backend restart failed: ${(error.stdout || error.message || '').toString().trim().slice(-200)}`);
      }
      await sleep(settleMs, signal);
    },
  });
}

export function createDatabaseWriteCapability({ backend, spacetime, dbName, expand,
  exec = execFileSync, mongoContainer = process.env.MONGO_CONTAINER ?? 'stack-bench-mongodb',
  postgresContainer = process.env.POSTGRES_CONTAINER ?? 'stack-bench-postgres' }) {
  return Object.freeze({
    setStock(input) {
      const item = expand(input.item);
      const warehouse = expand(input.warehouse);
      const quantity = Number(input.quantity);
      if (!Number.isInteger(quantity)) fail(`dbSetStock: quantity must be a whole number, got ${input.quantity}`);
      try {
        return executeStackCapability(STACK_ADAPTER_REGISTRY.get(backend),
          'database-write', 'set-stock', {
            item, warehouse, quantity, spacetime, dbName, exec,
            containers: { mongodb: mongoContainer, postgres: postgresContainer },
          });
      } catch (error) {
        if (error instanceof StackCapabilityUnsupportedError) {
          inconclusive(`direct stock writes do not support backend ${backend ?? '<unset>'}`);
        }
        throw error;
      }
    },
  });
}

export const LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS = Object.freeze({
  clickConcurrently,
  closeClient: browserApplicationBoundary(closeClient),
  dbSetStock,
  freshClient: browserApplicationBoundary(freshClient),
  openClient: browserApplicationBoundary(openClient),
  race,
  replayConcurrently: browserApplicationBoundary(replayConcurrently),
  restartBackend,
  sendConcurrently,
  setOffline: browserApplicationBoundary(setOffline),
  startAppServer,
  stopAppServer,
});

if (Object.keys(LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS).sort().join('\0')
  !== LIFECYCLE_CONCURRENCY_ACTION_IDS.join('\0')) {
  throw new Error('lifecycle/concurrency registry does not match its declared action ids');
}
