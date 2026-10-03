import { createHash } from 'node:crypto';
import type { BrowserContext } from 'playwright';
import { crossOriginPost } from './cross-origin-request.js';
import { actionImplementation } from './action-contract.js';
import type { Operation } from './action-findings.js';
import {
  actorFor,
  fail,
  inconclusive,
  routeProofs,
  transportFor,
} from './actor-action-runtime.js';
import type {
  ActionCall,
  Actor,
  ActorActionArguments,
  ActorCapabilities,
  HeaderRecord,
  TransportActorCapabilities,
} from './actor-action-runtime.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import {
  browserCredentials,
  bindBrowserRequest,
  tamperedSessionCredentials,
  namedActionRequest,
  classifyNamedActionResponse,
} from './named-action-runtime.js';
import type {
  NamedAction,
  NamedActionsCapability,
} from './named-action-runtime.js';

interface CallActionInput {
  readonly action: string;
  readonly actor: string;
  readonly authentication?: 'actor' | 'none' | 'optional' | 'session-control' | 'tampered-session';
  readonly browserOrigin?: 'same-site' | 'cross-site';
  readonly from?: string;
  readonly input?: {
    readonly attribute: string;
    readonly contains?: string;
    readonly testid: string;
    // The parameters the attribute supplies; the rest keep the step's arguments.
    readonly fields?: readonly string[];
    readonly overrides?: Readonly<Record<string, {
      readonly actor: string;
      readonly testid: string;
      readonly contains?: string;
      readonly attribute: string;
    }>>;
  };
  readonly namedAction?: NamedAction;
  readonly settleMs?: number;
}

interface OutcomeInput {
  readonly actor: string;
  readonly outcome: 'accepted' | 'refused' | 'validation-refused' | 'completed' | 'application-refused';
  readonly routeProvenBy?: string;
}

interface ConcurrentCallInput {
  readonly action: string;
  readonly actors: readonly string[];
  readonly namedAction?: NamedAction;
  readonly input?: CallActionInput['input'];
  readonly from?: string;
  readonly args?: readonly unknown[];
  readonly body?: unknown;
  readonly settleMs?: number;
  readonly requests?: number;
  readonly requestTimeoutMs?: number;
  readonly delayMs?: number;
  readonly alongside?: readonly Omit<ConcurrentCallInput, 'alongside' | 'settleMs'>[];
}

interface ConcurrentOutcomeInput { readonly accepted?: number }

interface NamedActionCapabilities extends ActorCapabilities {
  readonly 'named-actions': NamedActionsCapability;
}

type NamedArguments<Input> = ActorActionArguments<Input, NamedActionCapabilities>;

type NamedTransportCapabilities = NamedActionCapabilities & TransportActorCapabilities;

type NamedTransportArguments<Input> =
  ActorActionArguments<Input, NamedTransportCapabilities>;

async function readActionValues(capabilities: NamedActionCapabilities, source: Actor, action: NamedAction,
  input: { action: string; input: NonNullable<CallActionInput['input']> }, within: number, signal: AbortSignal) {
  if (!Array.isArray(action.params) || action.params.length === 0) {
    inconclusive('action-without-parameters', { action: input.action });
  }

  const named = capabilities['named-actions'];
  const deadline = named.now() + within;
  const readAttribute = async (actor: Actor, selector: { testid: string; contains?: string; attribute: string }) => {
    const target = actor.loc(selector.testid, { contains: selector.contains });
    const missing = (): never => fail('interface-missing', {
      control: selector.testid, attribute: selector.attribute, action: input.action,
    });
    const remaining = () => {
      signal.throwIfAborted();
      const ms = deadline - named.now();
      if (ms <= 0) missing();
      return Math.max(1, ms);
    };
    try {
      // A control that never appears is not one that lacks the attribute.
      try { await target.waitFor({ state: 'attached', timeout: remaining() }); }
      catch (error) {
        signal.throwIfAborted();
        if (error instanceof Error && error.name === 'TimeoutError' && named.now() >= deadline) {
          fail('page-timeout', { control: selector.testid });
        }
        throw error;
      }
      for (;;) {
        const value = await target.getAttribute(selector.attribute, { timeout: remaining() });
        signal.throwIfAborted();
        if (value !== null && value !== '') return value;
        await named.sleep(Math.min(100, remaining()), signal);
      }
    } catch (error) {
      signal.throwIfAborted();
      if (error instanceof Error && error.name === 'TimeoutError' && named.now() >= deadline) missing();
      throw error;
    }
  };
  const raw = await readAttribute(source, input.input);
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
  const fields = input.input.fields ?? expected;
  const absent = fields.filter(name => !Object.hasOwn(supplied, name));
  if (input.input.fields && absent.length) {
    fail('interface-invalid', { action: input.action, attribute: input.input.attribute, missing: absent });
  }
  const defaults = action.args ?? [];
  const actionValues = Object.fromEntries(expected.map((name, index) =>
    [name, fields.includes(name) && Object.hasOwn(supplied, name) ? supplied[name] : defaults[index]]));
  for (const [name, override] of Object.entries(input.input.overrides ?? {})) {
    if (!expected.includes(name)) invalid(`override parameter ${name} is not declared`);
    actionValues[name] = await readAttribute(actorFor(capabilities, override.actor), override);
  }
  const missing = expected.filter(name => actionValues[name] === undefined);
  if (missing.length) {
    fail('interface-invalid', { action: input.action, attribute: input.input.attribute, missing });
  }

  return actionValues;
}

async function callAction({ input, capabilities, signal }: NamedTransportArguments<CallActionInput>) {
  const caller = actorFor(capabilities, input.actor);
  const source = actorFor(capabilities, input.from ?? input.actor);
  const named = capabilities['named-actions'];
  const transport = transportFor(capabilities);
  const action = input.namedAction ?? named.resolve(input.action);
  if (!action) inconclusive('unknown-action', { action: input.action });
  const fixedBody = action.params?.length && action.params.every(param => param.in === 'body')
    && action.args?.length === action.params.length;
  if (!input.input && !fixedBody && (action.params?.length || action.args?.length)) {
    inconclusive('unresolved-action', { action: input.action });
  }
  const actionValues = input.input
    ? await readActionValues(capabilities, source, action, { action: input.action, input: input.input }, transport.defaultWithin, signal)
    : Object.fromEntries((action.params ?? []).map((param, index) => [param.name, action.args![index]]));

  const request = namedActionRequest(named, action, { values: actionValues });
  if (!request?.url) inconclusive('unresolved-action', { action: input.action });
  let credentials: HeaderRecord = {};
  if (input.authentication !== 'none') {
    const actorCredentials = await browserCredentials(caller, request.url, input.authentication === 'optional');
    if (!actorCredentials && input.authentication !== 'optional') inconclusive('no-session', { actor: caller.name, action: input.action });
    credentials = actorCredentials ?? {};
  }
  const bindRequest = bindBrowserRequest(caller, request, credentials);
  // Keep only a digest in actor state, never a second copy of the session secret.
  const fingerprint = (headers: HeaderRecord) => createHash('sha256').update(JSON.stringify([
    input.action, request.url, request.method ?? 'POST', request.body,
    Object.entries(headers).map(([key, value]) => [key.toLowerCase(), value]).sort(),
  ])).digest('hex');
  if (input.browserOrigin) {
    if (!caller.actionCall?.accepted || caller.actionCall.requestFingerprint !== fingerprint(credentials)) {
      inconclusive('replay-unavailable', { actor: caller.name,
        detail: 'cross-origin probing requires a successful identical request with the current credentials' });
    }
    caller.actionCall = undefined; // An opaque response cannot establish semantic acceptance or refusal.
    if (!caller.context.newPage || !caller.context.grantPermissions) {
      inconclusive('replay-unavailable', { actor: caller.name, detail: 'cross-origin probing requires the real browser context' });
    }
    const result = await crossOriginPost(caller.context as Pick<BrowserContext, 'newPage' | 'grantPermissions'>,
      { ...request, url: request.url }, input.browserOrigin, signal);
    await transport.sleep(input.settleMs ?? 2000, signal);
    return { action: input.action, ...result };
  }
  if (input.authentication === 'session-control') {
    if (!caller.actionCall?.accepted || caller.actionCall.requestFingerprint !== fingerprint(credentials)) {
      inconclusive('replay-unavailable', { actor: caller.name,
        detail: 'session isolation requires a successful identical request with the full current credentials' });
    }
  }
  if (input.authentication === 'session-control' || input.authentication === 'tampered-session') {
    // A bearer-only positive control must succeed before a cookie-free forgery is meaningful.
    if (Object.entries(credentials).some(([key, value]) => /^authorization$/i.test(key) && /^Bearer /i.test(value))) {
      credentials = Object.fromEntries(Object.entries(credentials).filter(([key]) => !/^cookie$/i.test(key)));
    }
    if (input.authentication === 'session-control') tamperedSessionCredentials(credentials, caller.name);
  }
  const requestFingerprint = fingerprint(credentials);
  if (input.authentication === 'tampered-session') {
    if (!caller.actionCall?.accepted || caller.actionCall.requestFingerprint !== requestFingerprint) {
      inconclusive('replay-unavailable', { actor: caller.name,
        detail: 'tampering requires a successful identical request with the current credential' });
    }
    credentials = tamperedSessionCredentials(credentials, caller.name);
  }
  signal.throwIfAborted();
  let status = 0;
  let classified = classifyNamedActionResponse(named, request, { status, text: '' });
  try {
    const response = await named.fetch(request.url, {
      method: request.method ?? 'POST', ...bindRequest({ 'Content-Type': 'application/json', ...credentials }), signal,
    });
    const text = await response.text();
    status = response.status;
    caller.record(text);
    classified = classifyNamedActionResponse(named, request, { status, text });
  } catch { /* No complete response: never infer rejection from transport loss. */ }
  caller.actionCall = {
    action: input.action,
    ...classified,
    accepted: classified.ok,
    status,
    url: request.url,
    method: request.method ?? 'POST',
    operation: { reducer: action.reducer ?? null, path: action.path ?? null,
      method: action.method ?? 'POST' },
    requestFingerprint: input.authentication === 'tampered-session' ? undefined : requestFingerprint,
  };
  if (caller.actionCall.accepted) routeProofs(capabilities).add(input.action);
  await transport.sleep(input.settleMs ?? 2000, signal);
  if (input.authentication === 'session-control' && !caller.actionCall.accepted) {
    inconclusive('replay-unavailable', { actor: caller.name,
      detail: `the isolated session credential did not complete a successful control purchase (status ${status})` });
  }
  return { action: input.action, accepted: caller.actionCall.accepted,
    status: caller.actionCall.status, responseContract: caller.actionCall.responseContract,
    refusalKind: caller.actionCall.refusalKind, applicationRejected: caller.actionCall.applicationRejected };
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
  if (call.complete === false) inconclusive('transport-incomplete', {});
  const http = !call.responseContract?.startsWith('convex-');
  if (input.outcome === 'completed' || input.outcome === 'application-refused' || input.outcome === 'refused') {
    const proof = input.routeProvenBy === undefined ? null
      : actorFor(capabilities, input.routeProvenBy).actionCall;
    // A refusal measures authorization only when the same operation was proven
    // to work for an authorized request; otherwise it may be a malformed call.
    const routeProven = proof?.accepted === true && proof.action === call.action
      || routeProofs(capabilities).has(call.action);
    const deliberateRefusal = call.refusalKind === 'access' || (http && [400, 401, 403, 409, 422].includes(call.status))
      || (http && call.status === 404 && routeProven)
      || call.applicationRejected === true || call.refusalKind === 'validation';
    if (call.accepted && input.outcome !== 'completed') {
      fail('call-accepted', { action: call.action, actor: actor.name, status, required: 'refused' });
    }
    if (!call.accepted && !deliberateRefusal) {
      fail('call-error', { action: call.action, actor: actor.name, status,
        required: input.outcome === 'refused' ? 'refused' : 'validation-refused', operation: missingOperation(call) });
    }
    if (!call.accepted && input.outcome !== 'completed' && !routeProven) {
      inconclusive('route-unproven', { actor: actor.name, action: call.action });
    }
  } else if (input.outcome === 'accepted') {
    if (!call.accepted) {
      fail('call-refused', { action: call.action, actor: actor.name, status,
        operation: missingOperation(call) });
    }
  } else if (input.outcome === 'validation-refused') {
    if (call.accepted) {
      fail('call-accepted', { action: call.action, actor: actor.name, status, required: 'validation-refused' });
    }
    if (!(http && [400, 409, 422].includes(call.status)) && call.applicationRejected !== true && call.refusalKind !== 'validation') {
      fail('call-error', { action: call.action, actor: actor.name, status,
        required: 'validation-refused', operation: missingOperation(call) });
    }
  }
  transport.verification.verified(
    `${actor.name}: server ${call.accepted ? 'accepted' : 'refused'} `
      + `action "${call.action}" (HTTP ${call.status})`);
  return { action: call.action, outcome: input.outcome, status: call.status,
    classification: 'verified' };
}

async function callConcurrently({ input, capabilities, signal }: NamedArguments<ConcurrentCallInput>) {
  const named = capabilities['named-actions'];
  const prepared: Array<{ name: string; credentials: HeaderRecord; action: string;
    values: Readonly<Record<string, unknown>>; request: NonNullable<ReturnType<typeof namedActionRequest>>; body?: string | null;
    delayMs: number; timeoutMs: number }> = [];
  for (const group of [input, ...(input.alongside ?? [])]) {
    const action = group.namedAction ?? named.resolve(group.action);
    if (!action) inconclusive('unknown-action', { action: group.action });
    const values = group.input ? await readActionValues(
      capabilities, actorFor(capabilities, group.from ?? group.actors[0]!), action,
      { action: group.action, input: group.input }, group.requestTimeoutMs ?? 30000, signal) : undefined;
    const request = namedActionRequest(named, action, values === undefined ? group : { values });
    if (!request?.url) inconclusive('unresolved-action', { action: group.action });
    const actors: Array<{ name: string; credentials: HeaderRecord; body?: string | null }> = [];
    for (const name of group.actors) {
      const actor = actorFor(capabilities, name);
      const credentials = await browserCredentials(actor, request.url);
      if (!credentials) inconclusive('no-session', { actor: name, action: group.action });
      const bound = bindBrowserRequest(actor, request, credentials)();
      actors.push({ name, credentials: bound.headers, body: bound.body });
    }
    for (let index = 0; index < (group.requests ?? actors.length); index++) {
      prepared.push({ ...actors[index % actors.length]!, action: group.action, values: values ?? {}, request,
        delayMs: group.delayMs ?? 0, timeoutMs: group.requestTimeoutMs ?? 30000 });
    }
  }
  const started = named.now();
  const outcomes = await Promise.all(prepared.map(async (preparedActor, index) => {
    const request = preparedActor.request;
    const scheduledAtMs = named.now();
    let startedAtMs = scheduledAtMs;
    let dispatched = false;
    const timeout = AbortSignal.timeout(preparedActor.timeoutMs);
    const requestSignal = AbortSignal.any([signal, timeout]);
    let response;
    try {
      if (preparedActor.delayMs) await named.sleep(preparedActor.delayMs, requestSignal);
      requestSignal.throwIfAborted();
      startedAtMs = named.now();
      dispatched = true;
      const reply = await named.fetch(request.url!, {
        method: request.method ?? 'POST',
        headers: { 'Content-Type': 'application/json', ...preparedActor.credentials },
        body: preparedActor.body,
        signal: requestSignal,
      });
      const text = await reply.text();
      const classified = classifyNamedActionResponse(named, request, { status: reply.status, text });
      response = { status: reply.status, ...classified,
        text: classified.ok ? '' : text.slice(0, 120), transport: 'response' as const };
    } catch {
      response = { status: 0, ok: false,
        text: signal.aborted ? 'request cancelled' : timeout.aborted ? 'request timed out' : 'request transport failed',
        transport: signal.aborted ? 'cancelled' as const : timeout.aborted ? 'timeout' as const : 'error' as const };
    }
    const completedAtMs = named.now();
    return { ...response, name: preparedActor.name, requestIndex: index + 1,
      action: preparedActor.action, values: preparedActor.values, delayMs: preparedActor.delayMs, scheduledAtMs, dispatched,
      startedAtMs, completedAtMs, durationMs: Math.max(0, completedAtMs - startedAtMs) };
  }));
  const result = { action: input.action, fired: outcomes.length, ms: named.now() - started, outcomes,
    responses: outcomes.filter(outcome => outcome.transport === 'response').length,
    transportErrors: outcomes.filter(outcome => outcome.transport === 'error').length,
    timeouts: outcomes.filter(outcome => outcome.transport === 'timeout').length,
    cancelled: outcomes.filter(outcome => outcome.transport === 'cancelled').length,
    timingScope: 'client request dispatch through response; not server execution overlap' };
  named.lastCalls.set(result);
  // Return the drained history even on cancellation. executeAction retains it
  // with the interrupted verdict; cancellation must never turn into a pass.
  if (!signal.aborted) {
    try { await named.sleep(input.settleMs ?? 3000, signal); }
    catch (error) { if (!signal.aborted) throw error; }
  }
  return result;
}

async function expectCallOutcomes({ input, capabilities }: NamedArguments<ConcurrentOutcomeInput>) {
  const result = capabilities['named-actions'].lastCalls.get();
  if (!result) inconclusive('assertion-without-action', { action: 'callConcurrently' });
  if (result.outcomes.length !== result.fired || result.outcomes.some(outcome => outcome.status === 0 || outcome.complete === false)) {
    inconclusive('transport-incomplete', {});
  }
  for (const outcome of result.outcomes) {
    if (!outcome.ok && !(!outcome.responseContract?.startsWith('convex-') && [400, 409, 422].includes(outcome.status))
      && outcome.applicationRejected !== true && outcome.refusalKind !== 'validation') {
      fail('call-error', { action: result.action, actor: outcome.name,
        status: outcome.status || null, required: 'validation-refused', operation: null });
    }
  }
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
