import { createActionRegistry } from './action-contract.js';
import type { ActionCategory, ActionImplementation, ActionPlugin } from './action-contract.js';
import { ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS }
  from './actor-transport-action-executors.js';
import { BROWSER_ACTION_IMPLEMENTATIONS } from './browser-action-executors.js';
import { RUNTIME_ACTION_IMPLEMENTATIONS } from './runtime-action-executors.js';
import { confirmCheckout, crashCheckout, expectCrashCheckout } from './crash-action-executors.js';
import { prepareResponseLoss, loseCheckoutResponse } from './response-loss-action-executors.js';
import { ACTION_DEFINITIONS, ACTION_IDS,
  compileActionInput } from '../composition/definition-compiler.js';
import type { ActionId } from '../composition/definition-compiler.js';

const ACTION_CATEGORY = {
  armScriptCanary: 'browser-observation',
  expectNoScriptExecution: 'browser-observation',
  callAction: 'transport',
  callConcurrently: 'concurrency',
  confirmCheckout: 'transport',
  prepareResponseLoss: 'transport',
  loseCheckoutResponse: 'transport',
  crashCheckout: 'lifecycle',
  expectCrashCheckout: 'database',
  clearInput: 'browser-interaction',
  click: 'browser-interaction',
  clickConcurrently: 'concurrency',
  closeClient: 'browser-interaction',
  createRoom: 'browser-interaction',
  dbSetStock: 'database',
  ensureSignedIn: 'browser-interaction',
  enterRoom: 'browser-interaction',
  expect: 'browser-observation',
  expectActorsWith: 'browser-observation',
  expectActionOutcome: 'transport',
  expectAgreement: 'browser-observation',
  expectAllPresent: 'browser-observation',
  expectCallOutcomes: 'concurrency',
  expectElementCount: 'browser-observation',
  expectForgeryRejected: 'transport',
  expectNotReceived: 'transport',
  expectNumber: 'browser-observation',
  dbRecordStock: 'database',
  dbRecordCheckout: 'database',
  dbExpectCheckout: 'database',
  dbExpectOperation: 'database',
  dbExpectCancellation: 'database',
  dbExpectNoPurchase: 'database',
  dbExpectPurchase: 'database',
  dbExpectPurchases: 'database',
  dbExpectPurchaseCount: 'database',
  dbExpectStock: 'database',
  expectOrderMatches: 'browser-observation',
  expectSequence: 'browser-observation',
  expectReceived: 'transport',
  expectReplayCompleted: 'transport',
  expectReplayRejected: 'transport',
  expectStable: 'browser-observation',
  expectUnavailable: 'browser-observation',
  fill: 'browser-interaction',
  forgeWrite: 'transport',
  freshClient: 'browser-interaction',
  openClient: 'browser-interaction',
  openItem: 'browser-interaction',
  pressKey: 'browser-interaction',
  race: 'concurrency',
  recordNumber: 'browser-observation',
  recordTime: 'timing',
  expectElapsed: 'timing',
  reload: 'browser-interaction',
  replayAs: 'transport',
  replayConcurrently: 'concurrency',
  restartBackend: 'lifecycle',
  runScript: 'application-process',
  send: 'browser-interaction',
  sendConcurrently: 'concurrency',
  sendMany: 'browser-interaction',
  setOffline: 'browser-interaction',
  signIn: 'browser-interaction',
  signUp: 'browser-interaction',
  startAppServer: 'lifecycle',
  stopAppServer: 'lifecycle',
  typeInto: 'browser-interaction',
  wait: 'timing',
  waitUntilAbsent: 'browser-observation',
} as const satisfies Record<ActionId, ActionCategory>;

interface CategoryPolicy {
  readonly timeoutMs: number;
  readonly capabilities: readonly string[];
  readonly sensitivity: readonly string[];
}

const CATEGORY_POLICY = {
  'application-process': { timeoutMs: 90_000,
    capabilities: ['application-files', 'subprocess'],
    sensitivity: ['user-content', 'filesystem-path', 'process-output'] },
  'browser-interaction': { timeoutMs: 60_000,
    capabilities: ['actors', 'browser-interaction'], sensitivity: ['user-content'] },
  'browser-observation': { timeoutMs: 300_000,
    capabilities: ['actors', 'browser-observation'], sensitivity: [] },
  concurrency: { timeoutMs: 120_000,
    capabilities: ['actors', 'concurrency'], sensitivity: [] },
  database: { timeoutMs: 90_000,
    capabilities: ['clock', 'database-write'], sensitivity: [] },
  // Must exceed the bounded stop, start, and readiness sequence.
  lifecycle: { timeoutMs: 900_000,
    capabilities: ['backend-lifecycle'], sensitivity: ['network-address'] },
  timing: { timeoutMs: 360_000,
    capabilities: ['actors', 'clock'], sensitivity: [] },
  transport: { timeoutMs: 60_000,
    capabilities: ['actors', 'transport-observation'],
    sensitivity: ['user-content', 'network-address'] },
} as const satisfies Record<ActionCategory, CategoryPolicy>;

const ACTION_CAPABILITY_OVERRIDES: Partial<Record<ActionId, readonly string[]>> = {
  dbRecordStock: ['database-read', 'browser-observation'],
  dbRecordCheckout: ['database-read'],
  dbExpectCheckout: ['database-read', 'actors'],
  dbExpectOperation: ['database-read', 'named-actions'],
  dbExpectCancellation: ['database-read'],
  dbExpectNoPurchase: ['database-read'],
  dbExpectPurchase: ['actors', 'database-read', 'browser-observation'],
  dbExpectPurchases: ['database-read', 'named-actions'],
  dbExpectPurchaseCount: ['database-read'],
  recordTime: ['browser-observation'],
  expectElapsed: ['browser-observation'],
  wait: ['actors', 'clock', 'browser-observation'],
  dbExpectStock: ['database-read', 'browser-observation', 'clock'],
  callAction: ['actors', 'named-actions', 'transport-observation'],
  callConcurrently: ['actors', 'named-actions'],
  crashCheckout: ['actors', 'named-actions', 'database-read', 'process-crash', 'browser-observation'],
  expectCrashCheckout: ['browser-observation'],
  confirmCheckout: ['actors', 'named-actions', 'database-read'],
  prepareResponseLoss: ['actors', 'response-loss'],
  loseCheckoutResponse: ['actors', 'response-loss', 'database-read', 'clock'],
  expectCallOutcomes: ['actors', 'named-actions'],
  replayAs: ['actors', 'named-actions', 'transport-observation'],
  forgeWrite: ['actors', 'named-actions', 'transport-observation'],
  startAppServer: ['application-lifecycle'],
  stopAppServer: ['application-lifecycle'],
};

const ACTION_SENSITIVITY_OVERRIDES: Partial<Record<ActionId, readonly string[]>> = {
  ensureSignedIn: ['credential'],
  signIn: ['credential'],
  signUp: ['credential'],
};

export const ACTION_IMPLEMENTATIONS = Object.freeze({
  confirmCheckout,
  prepareResponseLoss,
  loseCheckoutResponse,
  crashCheckout,
  expectCrashCheckout,
  ...ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS,
  ...BROWSER_ACTION_IMPLEMENTATIONS,
  ...RUNTIME_ACTION_IMPLEMENTATIONS,
} satisfies Record<ActionId, ActionImplementation>);

export function actionPlugin(id: string): ActionPlugin {
  if (!Object.hasOwn(ACTION_DEFINITIONS, id)) throw new Error(`cannot register unknown action ${id}`);
  const actionId = id as ActionId;
  const actionCategory = ACTION_CATEGORY[actionId];
  const policy = CATEGORY_POLICY[actionCategory];
  const capabilities = ACTION_CAPABILITY_OVERRIDES[actionId]
    ?? policy.capabilities;
  const sensitivity = ACTION_SENSITIVITY_OVERRIDES[actionId];
  return {
    id,
    version: '1.0.0',
    category: actionCategory,
    compile: (input: unknown) =>
      compileActionInput(input, { source: `action:${id}`, expectedAction: id }),
    capabilities,
    timeoutMs: policy.timeoutMs,
    // Recovery may be inside an existing 120-second synchronous Docker call.
    ...(actionId === 'crashCheckout' ? { cancellationDrainMs: 150_000 } : {}),
    sensitivity: [...(sensitivity ?? []), ...policy.sensitivity],
    execute: ACTION_IMPLEMENTATIONS[actionId],
  };
}

export const ACTION_REGISTRY = createActionRegistry(ACTION_IDS.map(actionPlugin), {
  expectedIds: ACTION_IDS,
});
