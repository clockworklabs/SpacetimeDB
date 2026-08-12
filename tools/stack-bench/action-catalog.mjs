import {
  ACTION_EVIDENCE_SCHEMA_VERSION,
  ACTION_INPUT_SCHEMA_VERSION,
  ACTION_PLUGIN_SCHEMA_VERSION,
  createActionRegistry,
} from './action-contract.mjs';
import { ACTION_IDS, compileActionInput } from './definition-compiler.mjs';

const OBSERVATIONS = new Set([
  'expect', 'expectActorsWith', 'expectAgreement', 'expectAllPresent', 'expectCallOutcomes',
  'expectElementCount', 'expectForgeryRejected', 'expectNotReceived', 'expectNumber',
  'expectOrderMatches', 'expectReceived', 'expectReplayRejected', 'expectStable', 'recordNumber',
]);
const CONCURRENCY = new Set([
  'callConcurrently', 'clickConcurrently', 'expectCallOutcomes', 'race', 'replayConcurrently',
  'sendConcurrently',
]);
const TRANSPORT = new Set([
  'forgeWrite', 'replayAs', 'expectForgeryRejected', 'expectNotReceived', 'expectReceived',
  'expectReplayRejected',
]);
const LIFECYCLE = new Set(['restartBackend', 'startAppServer', 'stopAppServer']);
const CREDENTIALS = new Set(['ensureRegistered', 'ensureSignedIn', 'signIn', 'signUp']);

function category(id) {
  if (LIFECYCLE.has(id)) return 'lifecycle';
  if (id === 'dbSetStock') return 'database';
  if (id === 'runScript') return 'application-process';
  if (id === 'wait') return 'timing';
  if (CONCURRENCY.has(id)) return 'concurrency';
  if (TRANSPORT.has(id)) return 'transport';
  if (OBSERVATIONS.has(id)) return 'browser-observation';
  return 'browser-interaction';
}

function capabilities(id, actionCategory) {
  if (id === 'dbSetStock') return ['database-write'];
  if (id === 'runScript') return ['application-files', 'subprocess'];
  if (id === 'restartBackend') return ['backend-lifecycle'];
  if (id === 'startAppServer' || id === 'stopAppServer') return ['application-lifecycle'];
  if (id === 'wait') return ['actors', 'clock'];
  if (id === 'callConcurrently' || id === 'expectCallOutcomes') return ['actors', 'named-actions'];
  if (actionCategory === 'transport') return ['actors', 'transport-observation'];
  if (actionCategory === 'concurrency') return ['actors', 'concurrency'];
  if (actionCategory === 'browser-observation') return ['actors', 'browser-observation'];
  return ['actors', 'browser-interaction'];
}

function deadline(actionCategory) {
  // Hosted restart control has several independently bounded Docker commands
  // and readiness windows. Its outer deadline must exceed their worst-case
  // sequence so a synchronous child cannot outlive a supposedly timed-out
  // action and keep mutating the environment in the background.
  if (actionCategory === 'lifecycle') return 900_000;
  if (actionCategory === 'database') return 90_000;
  if (actionCategory === 'timing') return 120_000;
  if (actionCategory === 'application-process') return 90_000;
  if (actionCategory === 'browser-observation' || actionCategory === 'concurrency') return 120_000;
  return 60_000;
}

function sensitivity(id, actionCategory) {
  const values = [];
  if (CREDENTIALS.has(id)) values.push('credential');
  if (['browser-interaction', 'transport', 'application-process'].includes(actionCategory)) {
    values.push('user-content');
  }
  if (actionCategory === 'transport' || actionCategory === 'lifecycle') values.push('network-address');
  if (actionCategory === 'application-process') values.push('filesystem-path', 'process-output');
  return values;
}

function label(id) {
  return id.replace(/([a-z0-9])([A-Z])/g, '$1 $2').toLowerCase();
}

function validObservation(value, seen = new Set()) {
  if (value === undefined || value === null || ['string', 'boolean'].includes(typeof value)) return true;
  if (typeof value === 'number') return Number.isFinite(value);
  if (typeof value !== 'object' || seen.has(value)) return false;
  const prototype = Object.getPrototypeOf(value);
  if (!Array.isArray(value) && prototype !== Object.prototype && prototype !== null) return false;
  seen.add(value);
  const valid = (Array.isArray(value) ? value : Object.values(value))
    .every(item => validObservation(item, seen));
  seen.delete(value);
  return valid;
}

export function legacyActionPlugin(id) {
  if (!ACTION_IDS.includes(id)) throw new Error(`cannot register unknown compatibility action ${id}`);
  const actionCategory = category(id);
  return {
    schemaVersion: ACTION_PLUGIN_SCHEMA_VERSION,
    id,
    version: '1.0.0',
    input: {
      schemaVersion: ACTION_INPUT_SCHEMA_VERSION,
      compile: input => compileActionInput(input, { source: `action:${id}`, expectedAction: id }),
    },
    capabilities: capabilities(id, actionCategory),
    deadline: { timeoutMs: deadline(actionCategory) },
    evidence: {
      schemaVersion: ACTION_EVIDENCE_SCHEMA_VERSION,
      type: `${actionCategory}-evidence`,
      validate: validObservation,
    },
    redaction: {
      sensitivity: sensitivity(id, actionCategory),
      fields: CREDENTIALS.has(id) ? ['password'] : id === 'runScript' ? ['args'] : [],
    },
    renderer: { label: label(id), category: actionCategory },
    // During the compatibility migration the implementation is supplied by
    // exact action id. The plugin sees that one function and its declared
    // capabilities, never the grader's mutable context object.
    execute: ({ input, capabilities: scoped, signal, implementation, attempt }) =>
      implementation({ input, capabilities: scoped, signal, attempt }),
  };
}

export const ACTION_REGISTRY = createActionRegistry(ACTION_IDS.map(legacyActionPlugin), {
  expectedIds: ACTION_IDS,
});
