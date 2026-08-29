import { execFileSync } from 'node:child_process';

import { ActionApplicationFailure, ActionInconclusive } from './action-contract.js';
import type {
  ActionImplementation,
  ActionImplementationArguments,
} from './action-contract.js';
import { evidenceDisposition } from '../evidence/check-evidence.js';
import type { CheckEvidenceStatus } from '../evidence/check-evidence.js';
import { replayHeaders } from './actor-transport-action-executors.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import { controlBackend } from '../runtime/backend-control.mjs';
import { harnessBrowserFailure, harnessProcessFailure } from '../evidence/harness-errors.js';
import { executeStackCapability, StackCapabilityUnsupportedError } from '../stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.mjs';
import { databaseContainerName } from '../stacks/database-containers.js';

type UnknownRecord = Record<string, unknown>;
type Sleep = (milliseconds: number, signal: AbortSignal) => Promise<void>;
type Exec = (file: string, args: readonly string[], options?: UnknownRecord) => unknown;

declare const navigator: { readonly onLine: boolean };

interface CapturedWrite {
  readonly body?: unknown;
  readonly headers: Readonly<Record<string, string>>;
  readonly method: string;
  readonly url: string;
}

interface Locator {
  click(options?: unknown): Promise<void>;
  isEnabled(): Promise<boolean>;
  waitFor(options?: unknown): Promise<void>;
}

interface Actor {
  readonly lastWrite?: CapturedWrite | null;
  readonly lastWrites?: Readonly<Record<string, CapturedWrite | undefined>>;
  readonly page: {
    readonly request: {
      fetch(url: string, options: UnknownRecord): Promise<{ status(): number }>;
    };
    close(): Promise<void>;
    context(): { setOffline(offline: boolean): Promise<void> };
    evaluate<Result>(callback: () => Result): Promise<Result>;
  };
  readonly writes?: readonly CapturedWrite[];
  loc(testid: string, options?: unknown): Locator;
}

interface ActionStep extends UnknownRecord {
  readonly do: string;
}

interface ConcurrencyCapability {
  readonly defaultWithin: number;
  dispatch(step: ActionStep, signal: AbortSignal): Promise<unknown>;
  expand(value: string | undefined): string | undefined;
  sleep: Sleep;
  testId(id: string): string;
}

interface LifecycleCapability {
  operate(mode: 'restart' | 'start' | 'stop', settleMs: number, signal: AbortSignal): Promise<void>;
}

interface LifecycleConcurrencyCapabilities {
  readonly actors: { get(name: string): Actor | undefined };
  readonly 'application-lifecycle': LifecycleCapability;
  readonly 'backend-lifecycle': LifecycleCapability;
  readonly 'browser-interaction': {
    readonly clients: {
      fresh(actor: Actor, actorName: string): Promise<string>;
      open(actor: Actor, settleMs: number, signal: AbortSignal): Promise<void>;
    };
    sleep: Sleep;
  };
  readonly clock: { sleep: Sleep };
  readonly concurrency: ConcurrencyCapability;
  readonly 'database-write': {
    setStock(input: SetStockInput): unknown | Promise<unknown>;
  };
}

interface ActionArguments<Input> {
  readonly input: Input;
  readonly capabilities: LifecycleConcurrencyCapabilities;
  readonly signal: AbortSignal;
}

interface ReplayConcurrentlyInput {
  readonly actors: readonly string[];
  readonly match?: string;
  readonly method?: string;
  readonly settleMs?: number;
}

interface LocatorScope {
  readonly contains?: string;
  readonly testid: string;
}

interface ClickTarget {
  readonly actor: string;
  readonly in?: LocatorScope;
}

interface ClickConcurrentlyInput {
  readonly actors: readonly string[];
  readonly in?: LocatorScope;
  readonly readyWithin?: number;
  readonly settleMs?: number;
  readonly targets?: readonly ClickTarget[];
  readonly testid: string;
  readonly within?: number;
}

interface RaceInput {
  readonly branches: readonly (readonly ActionStep[])[];
  readonly settleMs?: number;
}

interface ConcurrentSender {
  readonly actor: string;
  readonly count: number;
  readonly delayMs?: number;
  readonly prefix: string;
}

interface SendConcurrentlyInput {
  readonly delayMs?: number;
  readonly senders: readonly ConcurrentSender[];
}

interface SettleInput { readonly settleMs?: number }
interface ActorInput { readonly actor: string; readonly settleMs?: number }
interface OfflineInput extends ActorInput { readonly offline?: boolean }
interface SetStockInput {
  readonly item: string;
  readonly quantity: number;
  readonly settleMs: number;
  readonly warehouse: string;
}

interface ProcessErrorShape {
  readonly classification?: unknown;
  readonly code?: unknown;
  readonly message?: unknown;
  readonly status?: unknown;
  readonly stderr?: unknown;
  readonly stdout?: unknown;
}

interface NestedActionEvidence {
  readonly status?: unknown;
  readonly summary?: string | null;
}

const errorShape = (error: unknown): ProcessErrorShape =>
  error !== null && typeof error === 'object' ? error as ProcessErrorShape : {};
const errorEvidence = (error: unknown): NestedActionEvidence | null => {
  if (error === null || typeof error !== 'object' || !('actionEvidence' in error)) return null;
  const evidence = error.actionEvidence;
  return evidence !== null && typeof evidence === 'object' ? evidence as NestedActionEvidence : null;
};

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

const fail = (message: string): never => { throw new ActionApplicationFailure(message); };
const inconclusive = (message: string): never => { throw new ActionInconclusive(message); };

export function databaseWriteFailureDetail(error: unknown): string {
  const value = errorShape(error);
  const details = [value.message, value.stdout, value.stderr]
    .map(value => Buffer.isBuffer(value) ? value.toString('utf8') : String(value ?? ''))
    .map(value => value.trim()).filter(Boolean)
    .filter((value, index, all) => all.indexOf(value) === index);
  return details.join(' | ').slice(-600) || 'unknown database-write failure';
}

async function dispatchNested(
  concurrency: ConcurrencyCapability,
  step: ActionStep,
  signal: AbortSignal,
): Promise<unknown> {
  try {
    return await concurrency.dispatch(step, signal);
  } catch (error) {
    const evidence = errorEvidence(error);
    const status = evidence?.status;
    const disposition = typeof status === 'string'
      ? evidenceDisposition(status as CheckEvidenceStatus)
      : null;
    if (disposition?.applicationFailure) fail(evidence?.summary ?? `${step.do} failed`);
    if (disposition?.outcomeKind === 'inconclusive') {
      inconclusive(evidence?.summary ?? `${step.do} was inconclusive`);
    }
    throw error;
  }
}

function actorFor(capabilities: LifecycleConcurrencyCapabilities, name: string): Actor {
  const actor = capabilities.actors.get(name);
  if (!actor) return fail(`unknown actor "${name}"`);
  return actor;
}

async function replayConcurrently(
  { input, capabilities, signal }: ActionArguments<ReplayConcurrentlyInput>,
) {
  const concurrency = capabilities.concurrency;
  const method = input.method ?? 'POST';
  const match = input.match;
  const pick = (actor: Actor): CapturedWrite | undefined | null => {
    if (match) {
      return [...(actor.writes ?? [])].reverse()
        .find(write => write.method === method && write.url.includes(match));
    }
    return actor.lastWrites?.[method] ?? actor.lastWrite;
  };
  const pending = input.actors.map(name => {
    const actor = actorFor(capabilities, name);
    return { actor, write: pick(actor) };
  }).filter((candidate): candidate is { actor: Actor; write: CapturedWrite } =>
    candidate.write !== null && candidate.write !== undefined);
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
    }).then(response => response.status(), error =>
      `error: ${String(errorShape(error).message ?? error).split('\n')[0]}`)));
  const answered = replies.filter(status => typeof status === 'number');
  if (answered.length < 2) {
    inconclusive(`only ${answered.length} of ${pending.length} replayed ${input.match ?? 'write'} requests `
      + `reached the server (responses: ${replies.join(', ')}), so the two never contended.`);
  }
  await concurrency.sleep(input.settleMs ?? 3000, signal);
  return { attempted: pending.length, answered: answered.length, replies };
}

async function clickConcurrently(
  { input, capabilities, signal }: ActionArguments<ClickConcurrentlyInput>,
) {
  const concurrency = capabilities.concurrency;
  const targets: readonly ClickTarget[] = input.targets ?? input.actors.map(actor => ({ actor }));
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
      .then(() => null, error =>
        `${target.actor}: ${String(errorShape(error).message ?? error).split('\n')[0]}`)));
  const failed = outcomes.filter(Boolean);
  if (failed.length) {
    inconclusive(`${failed.length} of ${targets.length} concurrent clicks on `
      + `${concurrency.testId(input.testid)} never dispatched, so nothing was actually contended — `
      + failed.join(' | '));
  }
  await concurrency.sleep(input.settleMs ?? 3000, signal);
  return { dispatched: targets.length };
}

async function race({ input, capabilities, signal }: ActionArguments<RaceInput>) {
  const concurrency = capabilities.concurrency;
  await Promise.all(input.branches.map(async branch => {
    for (const step of branch) await dispatchNested(concurrency, step, signal);
  }));
  await concurrency.sleep(input.settleMs ?? 2000, signal);
  return { branches: input.branches.length };
}

async function sendConcurrently(
  { input, capabilities, signal }: ActionArguments<SendConcurrentlyInput>,
) {
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

async function restartBackend({ input, capabilities, signal }: ActionArguments<SettleInput>) {
  await capabilities['backend-lifecycle'].operate('restart', input.settleMs ?? 10000, signal);
  return { operation: 'restart' };
}

async function startAppServer({ input, capabilities, signal }: ActionArguments<SettleInput>) {
  await capabilities['application-lifecycle'].operate('start', input.settleMs ?? 8000, signal);
  return { operation: 'start' };
}

async function stopAppServer({ input, capabilities, signal }: ActionArguments<SettleInput>) {
  await capabilities['application-lifecycle'].operate('stop', input.settleMs ?? 2000, signal);
  return { operation: 'stop' };
}

async function dbSetStock({ input, capabilities, signal }: ActionArguments<SetStockInput>) {
  let result: unknown;
  try {
    result = await capabilities['database-write'].setStock(input);
  } catch (error) {
    if (errorShape(error).classification || harnessProcessFailure(error)) throw error;
    fail(`dbSetStock failed: ${databaseWriteFailureDetail(error)}`);
  }
  await capabilities.clock.sleep(input.settleMs, signal);
  return result;
}

async function setOffline({ input, capabilities, signal }: ActionArguments<OfflineInput>) {
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

async function closeClient({ input, capabilities }: ActionArguments<ActorInput>) {
  await actorFor(capabilities, input.actor).page.close();
  return { closed: true };
}

async function openClient({ input, capabilities, signal }: ActionArguments<ActorInput>) {
  const actor = actorFor(capabilities, input.actor);
  await capabilities['browser-interaction'].clients.open(actor, input.settleMs ?? 4000, signal);
  return { opened: true };
}

async function freshClient({ input, capabilities }: ActionArguments<ActorInput>) {
  const actor = actorFor(capabilities, input.actor);
  const name = await capabilities['browser-interaction'].clients.fresh(actor, input.actor);
  return { actor: name };
}

interface LifecycleCapabilityOptions {
  readonly application?: boolean;
  readonly control?: (
    restartSpec: unknown,
    mode: 'restart' | 'start' | 'stop',
    options: { readonly signal: AbortSignal },
  ) => Promise<unknown>;
  readonly exec?: Exec;
  readonly restartCmd?: string;
  readonly restartSpec?: unknown;
  readonly sleep: Sleep;
}

export function createLifecycleCapability({ restartSpec, restartCmd, application = false,
  sleep, control = controlBackend, exec = execFileSync as unknown as Exec
}: LifecycleCapabilityOptions): LifecycleCapability {
  return Object.freeze({
    async operate(mode: 'restart' | 'start' | 'stop', settleMs: number, signal: AbortSignal) {
      if (!restartSpec && !restartCmd) {
        inconclusive(application
          ? 'no backend control supplied, cannot control the app server'
          : 'no backend control supplied, backend was never restarted');
      }
      try {
        if (restartSpec) await control(restartSpec, mode, { signal });
        else {
          const commandBase = restartCmd ?? inconclusive('no backend control command was supplied');
          const command = application ? `${commandBase} ${mode}` : commandBase;
          exec('bash', ['-c', command], { stdio: 'ignore', timeout: 300000 });
        }
      } catch (error) {
        const value = errorShape(error);
        if (value.status === 3) {
          inconclusive(application
            ? 'app-server control refused on this host'
            : 'backend restart refused — no benchmark-owned instance available');
        }
        // A generated app which cannot complete its own start/stop operation
        // has failed the application contract. Backend-control timeouts and
        // failures to launch the harness command still provide no app evidence.
        if (harnessProcessFailure(error) && !(application && value.code === 'ETIMEDOUT')) throw error;
        fail(application
          ? `could not ${mode} the app server: ${String(value.stdout || value.message || '').trim().slice(-160)}`
          : `backend restart failed: ${String(value.stdout || value.message || '').trim().slice(-200)}`);
      }
      await sleep(settleMs, signal);
    },
  });
}

interface DatabaseWriteCapabilityOptions {
  readonly backend?: string | null;
  readonly dbName: string;
  readonly exec?: Exec;
  readonly expand: (value: string) => string;
  readonly mongoContainer?: string;
  readonly postgresContainer?: string;
  readonly spacetime?: unknown;
}

export function createDatabaseWriteCapability({ backend, spacetime, dbName, expand,
  exec = execFileSync as unknown as Exec, mongoContainer = databaseContainerName('mongodb'),
  postgresContainer = databaseContainerName('postgres') }: DatabaseWriteCapabilityOptions) {
  return Object.freeze({
    setStock(input: SetStockInput): unknown {
      const item = expand(input.item);
      const warehouse = expand(input.warehouse);
      const quantity = Number(input.quantity);
      if (!Number.isInteger(quantity)) fail(`dbSetStock: quantity must be a whole number, got ${input.quantity}`);
      try {
        const adapter = backend ? STACK_ADAPTER_REGISTRY.get(backend) : undefined;
        if (!adapter) return inconclusive(
          `direct stock writes do not support backend ${backend ?? '<unset>'}`,
        );
        return executeStackCapability(adapter,
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

function contractLifecycleAction<Input, Result>(
  implementation: (arguments_: ActionArguments<Input>) => Result | Promise<Result>,
): ActionImplementation {
  return (arguments_: ActionImplementationArguments) => implementation({
    input: arguments_.input as Input,
    capabilities: arguments_.capabilities as unknown as LifecycleConcurrencyCapabilities,
    signal: arguments_.signal,
  });
}

function contractBrowserLifecycleAction<Input, Result>(
  implementation: (arguments_: ActionArguments<Input>) => Result | Promise<Result>,
): ActionImplementation {
  return contractLifecycleAction(browserApplicationBoundary(implementation));
}

export const LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS = Object.freeze({
  clickConcurrently: contractLifecycleAction(clickConcurrently),
  closeClient: contractBrowserLifecycleAction(closeClient),
  dbSetStock: contractLifecycleAction(dbSetStock),
  freshClient: contractBrowserLifecycleAction(freshClient),
  openClient: contractBrowserLifecycleAction(openClient),
  race: contractLifecycleAction(race),
  replayConcurrently: contractBrowserLifecycleAction(replayConcurrently),
  restartBackend: contractLifecycleAction(restartBackend),
  sendConcurrently: contractLifecycleAction(sendConcurrently),
  setOffline: contractBrowserLifecycleAction(setOffline),
  startAppServer: contractLifecycleAction(startAppServer),
  stopAppServer: contractLifecycleAction(stopAppServer),
});

if (Object.keys(LIFECYCLE_CONCURRENCY_ACTION_IMPLEMENTATIONS).sort().join('\0')
  !== LIFECYCLE_CONCURRENCY_ACTION_IDS.join('\0')) {
  throw new Error('lifecycle/concurrency registry does not match its declared action ids');
}
