import { actionImplementation } from './action-contract.js';
import type { Operation } from './action-findings.js';
import {
  actorFor,
  fail,
  inconclusive,
  transportFor,
} from './actor-action-runtime.js';
import type {
  ActionCall,
  ActorActionArguments,
  ActorCapabilities,
  HeaderRecord,
  TransportActorCapabilities,
} from './actor-action-runtime.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import {
  browserCredentials,
  capturedCredentials,
  namedActionRequest,
} from './named-action-runtime.js';
import type {
  NamedAction,
  NamedActionsCapability,
} from './named-action-runtime.js';

interface CallActionInput {
  readonly action: string;
  readonly actor: string;
  readonly authentication?: 'actor' | 'none';
  readonly from?: string;
  readonly input: {
    readonly attribute: string;
    readonly contains?: string;
    readonly testid: string;
  };
  readonly namedAction?: NamedAction;
  readonly settleMs?: number;
}

interface OutcomeInput {
  readonly actor: string;
  readonly outcome: 'accepted' | 'refused' | 'validation-refused';
  readonly routeProvenBy?: string;
}

interface ConcurrentCallInput {
  readonly action: string;
  readonly actors: readonly string[];
  readonly args?: readonly unknown[];
  readonly body?: unknown;
  readonly settleMs?: number;
}

interface ConcurrentOutcomeInput { readonly accepted?: number }

interface NamedActionCapabilities extends ActorCapabilities {
  readonly 'named-actions': NamedActionsCapability;
}

type NamedArguments<Input> = ActorActionArguments<Input, NamedActionCapabilities>;

type NamedTransportCapabilities = NamedActionCapabilities & TransportActorCapabilities;

type NamedTransportArguments<Input> =
  ActorActionArguments<Input, NamedTransportCapabilities>;

async function callAction({ input, capabilities, signal }: NamedTransportArguments<CallActionInput>) {
  const caller = actorFor(capabilities, input.actor);
  const source = actorFor(capabilities, input.from ?? input.actor);
  const named = capabilities['named-actions'];
  const transport = transportFor(capabilities);
  const action = input.namedAction ?? named.resolve(input.action);
  if (!action) inconclusive('unknown-action', { action: input.action });
  if (!Array.isArray(action.params) || action.params.length === 0) {
    inconclusive('action-without-parameters', { action: input.action });
  }

  const target = source.loc(input.input.testid, { contains: input.input.contains });
  await target.waitFor({ state: 'attached', timeout: transport.defaultWithin });
  const raw = await target.getAttribute(input.input.attribute);
  if (raw === null || raw === '') {
    fail('interface-missing', { control: input.input.testid, attribute: input.input.attribute,
      action: input.action });
  }
  const invalid = (detail: string): never => fail('interface-invalid',
    { action: input.action, attribute: input.input.attribute, detail });
  let values: unknown;
  try { values = JSON.parse(raw); }
  catch { invalid('not JSON'); }
  if (!values || typeof values !== 'object' || Array.isArray(values)) invalid('not an object');
  const supplied = values as Record<string, unknown>;
  const expected = action.params.map(param => param.name);
  const unexpected = Object.keys(supplied).filter(name => !expected.includes(name));
  if (unexpected.length) {
    fail('interface-invalid', { action: input.action, attribute: input.input.attribute,
      unexpected: unexpected.sort() });
  }
  const defaults = action.args ?? [];
  const actionValues = Object.fromEntries(expected.map((name, index) =>
    [name, Object.hasOwn(supplied, name) ? supplied[name] : defaults[index]]));
  const missing = expected.filter(name => actionValues[name] === undefined);
  if (missing.length) {
    fail('interface-invalid', { action: input.action, attribute: input.input.attribute, missing });
  }

  let credentials: HeaderRecord = {};
  if ((input.authentication ?? 'actor') === 'actor') {
    const actorCredentials = capturedCredentials(caller) ?? await browserCredentials(caller);
    if (!actorCredentials) inconclusive('no-session', { actor: caller.name, action: input.action });
    credentials = actorCredentials;
  }
  const request = namedActionRequest(named, action, { values: actionValues });
  if (!request?.url) inconclusive('unresolved-action', { action: input.action });
  const response = await named.fetch(request.url, {
    method: request.method ?? 'POST',
    headers: { 'Content-Type': 'application/json', ...credentials },
    body: request.body,
    signal,
  }).catch(error => ({ status: 0, ok: false, error: error.message }));
  caller.actionCall = {
    action: input.action,
    accepted: response.ok,
    status: response.status,
    url: request.url,
    method: request.method ?? 'POST',
    applicationRejected: (request.applicationRejectionStatuses ?? []).includes(response.status),
    operation: { reducer: action.reducer ?? null, path: action.path ?? null,
      method: action.method ?? 'POST' },
  };
  await transport.sleep(input.settleMs ?? 2000, signal);
  return { action: input.action, accepted: caller.actionCall.accepted,
    status: caller.actionCall.status };
}

// A 404 names the operation the application interface promised; any other
// status is the application's own answer.
function missingOperation(call: ActionCall): Operation | null {
  if (call.status !== 404 || !call.operation) return null;
  return { reducer: call.operation.reducer, path: call.operation.path, method: call.operation.method };
}

async function expectActionOutcome({ input, capabilities }: NamedTransportArguments<OutcomeInput>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const call = actor.actionCall;
  if (!call) inconclusive('assertion-without-action', { action: 'callAction' });
  const status = call.status || null;
  if (input.outcome === 'accepted') {
    if (!call.accepted) {
      fail('call-refused', { action: call.action, actor: actor.name, status,
        operation: missingOperation(call) });
    }
  } else if (input.outcome === 'validation-refused') {
    if (call.accepted) {
      fail('call-accepted', { action: call.action, actor: actor.name, status, required: 'validation-refused' });
    }
    if (![400, 409, 422].includes(call.status) && call.applicationRejected !== true) {
      fail('call-error', { action: call.action, actor: actor.name, status,
        required: 'validation-refused', operation: missingOperation(call) });
    }
  } else {
    if (call.accepted) {
      fail('call-accepted', { action: call.action, actor: actor.name, status, required: 'refused' });
    }
    const routeProof = input.routeProvenBy === undefined ? null
      : actorFor(capabilities, input.routeProvenBy).actionCall;
    const provenPrivateNotFound = call.status === 404 && routeProof?.accepted === true
      && routeProof.action === call.action;
    const deliberateRefusal = call.status === 401 || call.status === 403
      || provenPrivateNotFound || call.applicationRejected === true;
    if (!deliberateRefusal) {
      fail('call-error', { action: call.action, actor: actor.name, status,
        required: 'refused', operation: missingOperation(call) });
    }
  }
  transport.verification.verified(
    `${actor.name}: server ${input.outcome === 'accepted' ? 'accepted' : 'refused'} `
      + `action "${call.action}" (HTTP ${call.status})`);
  return { action: call.action, outcome: input.outcome, status: call.status,
    classification: 'verified' };
}

async function callConcurrently({ input, capabilities, signal }: NamedArguments<ConcurrentCallInput>) {
  const named = capabilities['named-actions'];
  const action = named.resolve(input.action);
  if (!action) inconclusive('unknown-action', { action: input.action });
  const prepared: Array<{ name: string; credentials: HeaderRecord }> = [];
  for (const name of input.actors) {
    const actor = actorFor(capabilities, name);
    const credentials = capturedCredentials(actor) ?? await browserCredentials(actor);
    if (!credentials) inconclusive('no-session', { actor: name, action: input.action });
    prepared.push({ name, credentials });
  }
  const request = named.request(action, input);
  const requestUrl = request?.url;
  if (!requestUrl) inconclusive('unresolved-action', { action: input.action });
  const started = named.now();
  const outcomes = await Promise.all(prepared.map(preparedActor =>
    named.fetch(requestUrl, {
      method: request.method ?? 'POST',
      headers: { 'Content-Type': 'application/json', ...preparedActor.credentials },
      body: request.body,
      signal,
    }).then(async response => ({ name: preparedActor.name, status: response.status, ok: response.ok,
      text: response.ok ? '' : (await response.text()).slice(0, 120) }))
      .catch(error => ({ name: preparedActor.name, status: 0, ok: false,
        text: String(error.message).slice(0, 120) }))));
  const result = { action: input.action, fired: prepared.length, ms: named.now() - started, outcomes };
  named.lastCalls.set(result);
  await named.sleep(input.settleMs ?? 3000, signal);
  return result;
}

async function expectCallOutcomes({ input, capabilities }: NamedArguments<ConcurrentOutcomeInput>) {
  const result = capabilities['named-actions'].lastCalls.get();
  if (!result) inconclusive('assertion-without-action', { action: 'callConcurrently' });
  const accepted = result.outcomes.filter(outcome => outcome.ok).length;
  if (input.accepted !== undefined && accepted !== input.accepted) {
    fail('concurrent-calls-mismatch', { action: result.action, expected: input.accepted, accepted,
      fired: result.fired,
      detail: `${result.outcomes.map(outcome => `${outcome.name}:${outcome.status}`).join(' ')} within ${result.ms}ms` });
  }
  return { accepted, fired: result.fired, outcomes: result.outcomes };
}

export const NAMED_ACTION_IMPLEMENTATIONS = Object.freeze({
  callAction: actionImplementation(browserApplicationBoundary(callAction)),
  callConcurrently: actionImplementation(browserApplicationBoundary(callConcurrently)),
  expectActionOutcome: actionImplementation(expectActionOutcome),
  expectCallOutcomes: actionImplementation(expectCallOutcomes),
});
