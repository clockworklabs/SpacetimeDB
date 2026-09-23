import { actionImplementation, ActionInconclusive } from './action-contract.js';
import {
  actorFor,
  fail,
  inconclusive,
  transportFor,
} from './actor-action-runtime.js';
import type {
  Actor,
  ActorActionArguments,
  CapturedWrite,
  ForgeResult,
  HeaderRecord,
  TransportActorCapabilities,
} from './actor-action-runtime.js';
import { browserApplicationBoundary, BROWSER_ACTION_IMPLEMENTATIONS } from './browser-action-executors.js';
import { harnessBrowserFailure } from '../evidence/harness-errors.js';
import { createParser } from 'eventsource-parser';
import { RUN_SCRIPT_ACTION_IMPLEMENTATION } from './application-process-action-executors.js';
import { CHAT_ACTION_IMPLEMENTATIONS } from './chat-action-executors.js';
import { NAMED_ACTION_IMPLEMENTATIONS } from './named-action-executors.js';
import {
  browserCredentials,
  bindBrowserRequest,
  REQUEST_CONTEXT_HEADER,
  namedActionRequest,
  classifyNamedActionResponse,
} from './named-action-runtime.js';
import type { NamedAction, NamedActionsCapability } from './named-action-runtime.js';
import type { NamedActionRequest } from './named-action-runtime.js';
import { capturedConvexMutation } from '../stacks/backends/convex-browser-session.js';
import { isFinding } from './action-findings.js';
import { isDeepStrictEqual } from 'node:util';

export { createNamedActionsCapability } from './named-action-runtime.js';
export type { ConcurrentCallResult } from './named-action-runtime.js';

interface ActorInput {
  readonly actor: string;
}

interface ForgeInput extends ActorInput {
  readonly field?: 'room' | 'identity';
  readonly fromActor?: string;
  readonly settleMs?: number;
  readonly text?: string;
  readonly value?: unknown;
}

interface ReplayInput extends ActorInput {
  readonly from: string;
  readonly match: string;
  readonly namedAction?: NamedAction;
  readonly namedTarget?: {
    readonly attribute: string;
    readonly contains?: string;
    readonly testid: string;
    readonly valueType?: 'number' | 'string';
  };
  readonly settleMs?: number;
  readonly swap?: { readonly find: string; readonly with: string };
}

interface ReceiveInput extends ActorInput {
  readonly contains: string;
  readonly within?: number;
}

type TransportCapabilities = TransportActorCapabilities & { readonly 'named-actions'?: NamedActionsCapability };

interface ReplayCapabilities extends TransportCapabilities {
  readonly 'named-actions': NamedActionsCapability;
}

type TransportArguments<Input extends ActorInput> =
  ActorActionArguments<Input, TransportCapabilities>;
type ReplayArguments = ActorActionArguments<ReplayInput, ReplayCapabilities>;

interface RepeatFormInput extends ActorInput {
  readonly match: string;
  readonly control: string;
  readonly replacement: string;
  readonly fields: readonly { testid: string; text: string }[];
  readonly submit: string;
  readonly settleMs?: number;
}

// Setup only: the scenario must independently check the stored effect after
// each write. Unsupported capture selects the original form BEFORE any replay.
async function repeatFormWrite({ input, capabilities, signal }:
  ActorActionArguments<RepeatFormInput, ReplayCapabilities & { readonly 'browser-interaction': unknown }>) {
  const actor = actorFor(capabilities, input.actor), named = capabilities['named-actions'];
  const transport = transportFor(capabilities);
  const find = transport.expand(input.match), replacement = transport.expand(input.replacement);
  let request: NamedActionRequest | null = null;
  let headers: HeaderRecord = {};
  const native = capturedConvexMutation(actor.page, find, replacement);
  const nativeControl = capturedConvexMutation(actor.page, transport.expand(input.control), replacement);
  if (native && isDeepStrictEqual(native, nativeControl)) {
    const candidate = namedActionRequest(named, { id: native.path, reducer: native.path }, { values: native.args });
    if (candidate?.url && candidate.responseContract === 'convex-mutation'
      && JSON.parse(candidate.body ?? '{}').path === native.path
      && [new URL(candidate.url).origin, new URL(actor.page.url()).origin].includes(native.origin)) {
      request = { ...candidate, applicationOrigin: native.origin };
    }
  } else {
    const capturedHttp = (name: string) => {
      const occurrences = (value: unknown): number => value === name ? 1 : value && typeof value === 'object'
        ? Object.values(value).reduce<number>((sum, child) => sum + occurrences(child), 0) : 0;
      const matches = actor.writes.filter(write => occurrences(write.body) > 0);
      const write = matches.length === 1 ? matches[0] : undefined;
      return write?.confirmed && occurrences(write.body) === 1
        && /^application\/json(?:;|$)/i.test(write.headers['content-type'] ?? '') ? { ...write,
        body: JSON.parse(JSON.stringify(write.body, (_key, value) => value === name ? replacement : value)),
        headers: replayHeaders(write) } : null;
    };
    const write = capturedHttp(find), control = capturedHttp(transport.expand(input.control));
    // The adapter recognizes Convex HTTP mutations only at the leased endpoint.
    const contract = write ? classifyNamedActionResponse(named, write, { status: 0, text: '' }).responseContract : null;
    if (write && isDeepStrictEqual(write, control)
      && (contract === 'convex-mutation'
        || contract === 'http' && new URL(write.url).origin === new URL(actor.page.url()).origin)) {
      request = { url: write.url, method: write.method, responseContract: contract,
        body: JSON.stringify(write.body) };
      headers = write.headers;
    }
  }
  let credentials: HeaderRecord | null = null;
  let bound: ReturnType<ReturnType<typeof bindBrowserRequest>> | null = null;
  try {
    credentials = request?.url ? await browserCredentials(actor, request.url) : null;
    if (request && credentials) bound = bindBrowserRequest(actor, request, credentials)(replayHeaders({ headers }, credentials));
  } catch (error) {
    if (!(error instanceof ActionInconclusive) || !isFinding(error.details.finding)
      || error.details.finding.kind !== 'replay-unavailable') throw error;
  }
  // Do not preserve old caller headers when current context cannot replace them.
  if (Object.keys(headers).some(key => REQUEST_CONTEXT_HEADER.test(key)
    && !Object.keys(credentials ?? {}).some(current => current.toLowerCase() === key.toLowerCase()))) request = null;
  if (!request?.url || !bound) {
    const observations = [];
    for (const field of input.fields) observations.push(await BROWSER_ACTION_IMPLEMENTATIONS.fill({
      input: { do: 'fill', actor: input.actor, ...field }, capabilities: { ...capabilities }, signal,
    }));
    observations.push(await BROWSER_ACTION_IMPLEMENTATIONS.click({
      input: { do: 'click', actor: input.actor, testid: input.submit, settleMs: input.settleMs }, capabilities: { ...capabilities }, signal,
    }));
    return { method: 'ui', reason: 'no uniquely confirmed supported write with current credentials', observations };
  }
  // Once sent, a lost response is unknown. It must never enter the form path.
  let response;
  try {
    if (request.responseContract === 'convex-mutation') {
      const reply = await named.fetch(request.url, { method: request.method ?? 'POST', ...bound, signal });
      response = { status: reply.status, text: await reply.text() };
    } else {
      const reply = await actor.page.request.fetch(request.url, { method: request.method, headers: bound.headers,
        data: bound.body, maxRetries: 0, maxRedirects: 0 });
      response = { status: reply.status(), text: await reply.text!() };
    }
  } catch (error) {
    if (harnessBrowserFailure(error)) throw error;
    inconclusive('transport-incomplete', {});
  }
  const classified = classifyNamedActionResponse(named, request, response);
  if (!classified.complete) inconclusive('transport-incomplete', {});
  if (!classified.ok) fail('call-error', { action: 'repeatFormWrite', actor: input.actor,
    status: response.status, required: 'accepted', operation: null });
  return { method: request.responseContract === 'convex-mutation' ? 'convex-mutation' : 'http',
    status: response.status, accepted: true };
}

const IDENTITY_FIELD = /^(user_?id|sender_?id|author_?id|from_?user|identity)$/i;
const CONTENT_FIELD = /^(content|text|message|body|msg)$/i;
const ROOM_FIELD = /^(room_?id|channel_?id|conversation_?id)$/i;
const ID_FIELD = /^(?:_?id|[A-Za-z][A-Za-z0-9_]*_?id)$/i;
const ID_KEY = /"(_?id|[A-Za-z][A-Za-z0-9_]*_?id)"\s*:\s*"?([A-Za-z0-9_-]{1,64})"?/gi;

function replayUnavailable(actor: Actor, reason: string): never {
  actor.replay = { inconclusive: true, reason };
  inconclusive('replay-unavailable', { actor: actor.name, detail: reason });
}
const tokenRe = (token: string): RegExp => new RegExp(
  `(?<![A-Za-z0-9_-])${token.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}(?![A-Za-z0-9_-])`, 'g');
const mentions = (value: string, token: string): boolean => tokenRe(token).test(value);
const swapToken = (value: string, from: string, to: string): string =>
  value.replace(tokenRe(from), to);

const normalizedIdKey = (key: string): string => key.replaceAll('_', '').toLowerCase();
const numericLiteral = (value: string): boolean =>
  /^-?(?:\d+\.?\d*|\.\d+)$/.test(value.trim());

function transportJson(chunk: string): unknown[] {
  const candidates = new Set([chunk]);
  for (const line of chunk.split(/\r?\n/)) {
    if (line.trim()) candidates.add(line.trim());
  }
  if (/^(?:data|event|id|retry):/m.test(chunk)) {
    const parser = createParser({
      maxBufferSize: 1024 * 1024,
      onEvent: ({ data }) => { if (data) candidates.add(data); },
    });
    try { parser.feed(`${chunk}\n\n`); }
    catch { /* Fall back to the other transport formats. */ }
  }
  const parsed: unknown[] = [];
  for (const candidate of candidates) {
    try {
      parsed.push(JSON.parse(candidate));
      continue;
    } catch { /* try a Socket.IO event packet */ }
    const socketEvent = candidate.match(/^42(?:\/[^,]+,)?\d*(\[.*\])$/s);
    if (!socketEvent) continue;
    const payload = socketEvent[1];
    if (!payload) continue;
    try { parsed.push(JSON.parse(payload)); } catch { /* malformed packet */ }
  }
  return parsed;
}

// Return nearby entity ids deepest first; fall back to text for streaming payloads.
interface DiscoveredId {
  readonly key: string;
  readonly value: string;
  readonly relationDepth: number | null;
  readonly proximity: number;
}

function discoverIds(actor: Actor, needle: string): DiscoveredId[] {
  const found = new Map<string, DiscoveredId>();
  const add = (
    key: string,
    value: unknown,
    relationDepth: number | null = null,
    proximity = Number.MAX_SAFE_INTEGER,
  ): void => {
    if (typeof value !== 'string' && typeof value !== 'number') return;
    const candidate = {
      key: normalizedIdKey(key), value: String(value), relationDepth, proximity,
    };
    const identity = `${candidate.key}\0${candidate.value}\0${relationDepth ?? ''}`;
    const previous = found.get(identity);
    if (!previous || proximity < previous.proximity) found.set(identity, candidate);
  };
  const visit = (value: unknown): number | null => {
    if (typeof value === 'string') return value.includes(needle) ? 0 : null;
    if (!value || typeof value !== 'object') return null;
    let nearest: number | null = null;
    const children = Array.isArray(value) ? value : Object.values(value);
    for (const child of children) {
      const distance = visit(child);
      if (distance !== null) nearest = nearest === null ? distance + 1 : Math.min(nearest, distance + 1);
    }
    if (nearest !== null && !Array.isArray(value)) {
      for (const [key, candidate] of Object.entries(value)) {
        if (ID_FIELD.test(key)) add(key, candidate, nearest);
      }
    }
    return nearest;
  };
  for (const chunk of actor.received ?? []) {
    const payloads = transportJson(chunk);
    for (const payload of payloads) visit(payload);
    if (payloads.length) continue;
    // Opaque transport text is only safe for entity labels represented as a
    // complete quoted value. Searching for a bare number such as "1" would
    // associate it with almost every id in a payload and fabricate a retarget.
    const quotedNeedle = JSON.stringify(needle);
    const needleOffsets = [];
    for (let index = chunk.indexOf(quotedNeedle); index !== -1;
      index = chunk.indexOf(quotedNeedle, index + 1)) {
      needleOffsets.push(index);
    }
    if (!needleOffsets.length) continue;
    for (const match of chunk.matchAll(ID_KEY)) {
      const proximity = Math.min(...needleOffsets.map(index => Math.abs((match.index ?? 0) - index)));
      const key = match[1];
      const value = match[2];
      if (key !== undefined && value !== undefined) add(key, value, null, proximity);
    }
  }
  return [...found.values()].sort((left, right) =>
    (left.relationDepth ?? Number.MAX_SAFE_INTEGER) - (right.relationDepth ?? Number.MAX_SAFE_INTEGER)
      || left.proximity - right.proximity);
}

export function replayHeaders(
  write: Pick<CapturedWrite, 'headers'>,
  overrides: HeaderRecord = {},
): HeaderRecord {
  const headers: HeaderRecord = {};
  for (const [key, value] of Object.entries(write.headers ?? {})) headers[key.toLowerCase()] = value;
  headers['content-type'] = 'application/json';
  for (const [key, value] of Object.entries(overrides)) headers[key.toLowerCase()] = value;
  for (const key of Object.keys(headers)) {
    if (/^(content-length|host|connection|transfer-encoding|accept-encoding)$/i.test(key)) {
      delete headers[key];
    }
  }
  return headers;
}

async function forgeWrite({ input, capabilities, signal }: TransportArguments<ForgeInput>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const write = [...(actor.writes ?? []), ...(actor.lastWrite ? [actor.lastWrite] : [])].reverse()
    .find(candidate => classifyNamedActionResponse(capabilities['named-actions'] ?? {}, candidate,
      { status: 0, text: '' }).responseContract !== 'convex-query');
  if (!write) {
    const websocket = actor.lastWsWrite;
    if (websocket) {
      const identityKey = Object.keys(websocket.body).find(key => IDENTITY_FIELD.test(key));
      actor.forge = identityKey
        ? { inconclusive: true, reason: `writes over WebSocket ("${websocket.event}") carrying a client-supplied "${identityKey}" — replay not attempted, treat as unverified` }
        : { inconclusive: true, reason: `writes over WebSocket ("${websocket.event}") with no top-level identity field — the captured payload does not prove where identity comes from` };
    } else {
      actor.forge = { inconclusive: true, reason: 'no write request observed at all — the test did not exercise anything' };
    }
    return { attempted: false, classification: 'unverified' };
  }
  const envelope = structuredClone(write.body ?? {});
  const native = classifyNamedActionResponse(capabilities['named-actions'] ?? {}, write,
    { status: 0, text: '' }).responseContract.startsWith('convex-');
  const argumentsOf = (body: Record<string, unknown>): Record<string, unknown> | null => {
    if (!native) return body;
    const args = Array.isArray(body.args) && body.args.length === 1 ? body.args[0] : body.args;
    return args && typeof args === 'object' && !Array.isArray(args) ? args as Record<string, unknown> : null;
  };
  const body = argumentsOf(envelope);
  if (!body) inconclusive('forgery-unverifiable', { actor: actor.name, detail: 'native write has no observable argument object' });
  const key = Object.keys(body).find(field => (input.field === 'room' ? ROOM_FIELD : IDENTITY_FIELD).test(field));
  if (!key) {
    actor.forge = { inconclusive: true,
      reason: `write body has no top-level identity field (${Object.keys(body).join(',') || 'empty'}) — the captured request does not prove where identity comes from` };
    return { attempted: false, classification: 'unverified' };
  }
  let value = input.value;
  if (input.fromActor) {
    const victim = actorFor(capabilities, input.fromActor);
    const targetField = input.field === 'room' ? ROOM_FIELD : IDENTITY_FIELD;
    const victimBody = argumentsOf(victim.lastWrite?.body ?? {});
    const victimKey = Object.keys(victimBody ?? {}).find(field => targetField.test(field));
    if (!victimKey) {
      const wsKey = victim.lastWsWrite && Object.keys(victim.lastWsWrite.body)
        .find(field => targetField.test(field));
      if (!wsKey) {
        actor.forge = { inconclusive: true,
          reason: `${input.fromActor} exposes no top-level ${input.field ?? 'identity'} field to use in the forgery` };
        return { attempted: false, classification: 'unverified' };
      }
      value = victim.lastWsWrite.body[wsKey];
    } else {
      value = victimBody?.[victimKey];
    }
  }
  body[key] = value;
  const contentKey = Object.keys(body).find(field => CONTENT_FIELD.test(field));
  if (contentKey && input.text) body[contentKey] = input.text;
  let forgeResult: ForgeResult;
  try {
    const response = await actor.page.request.fetch(write.url, {
      method: write.method, headers: replayHeaders(write), data: JSON.stringify(envelope),
    });
    const classified = classifyNamedActionResponse(capabilities['named-actions'] ?? {}, write,
      { status: response.status(), text: response.text ? await response.text() : '' });
    forgeResult = { ...classified, status: response.status(), accepted: classified.ok,
      tamperedField: key, reason: 'tampered request sent' };
  } catch (error) {
    if (harnessBrowserFailure(error)) throw error;
    forgeResult = { accepted: false, complete: false, inconclusive: true, tamperedField: key,
      reason: 'tampered request has no complete response' };
  }
  actor.forge = forgeResult;
  await transport.sleep(input.settleMs ?? 2000, signal);
  return {
    attempted: true,
    status: forgeResult.status,
    accepted: forgeResult.accepted,
    tamperedField: key,
  };
}

async function expectForgeryRejected({ input, capabilities }: TransportArguments<ActorInput>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const forge = actor.forge;
  if (!forge) inconclusive('assertion-without-action', { action: 'forgeWrite' });
  if (forge.inconclusive) {
    transport.verification.unverified(`${actor.name}: ${forge.reason}`);
    inconclusive('forgery-unverifiable', { actor: actor.name, detail: forge.reason });
  }
  // A server that ignores the tampered field accepts the request correctly;
  // the scenario's stored-effect check decides whether the forgery took effect.
  if (forge.accepted) {
    transport.verification.unverified(`${actor.name}: server accepted the tampered "${forge.tamperedField}"; its stored effect decides`);
    return { classification: 'unverified', status: forge.status };
  }
  if (forge.complete === false) inconclusive('transport-incomplete', {});
  if (forge.refusalKind !== 'access' && forge.refusalKind !== 'application'
    && !(forge.refusalKind === undefined && (forge.status === 401 || forge.status === 403))) {
    fail('forgery-error', { status: forge.status ?? null });
  }
  transport.verification.verified(
    `${actor.name}: server refused the tampered "${forge.tamperedField}" (HTTP ${forge.status})`);
  return { classification: 'verified', status: forge.status };
}

async function replayAs({ input, capabilities, signal }: ReplayArguments) {
  const actor = actorFor(capabilities, input.actor);
  const source = actorFor(capabilities, input.from);
  const transport = transportFor(capabilities);
  const needle = transport.expand(input.match).toLowerCase();
  const write = [...source.writes].reverse()
    .find(candidate => classifyNamedActionResponse(capabilities['named-actions'] ?? {}, candidate,
      { status: 0, text: '' }).responseContract !== 'convex-query'
      && `${candidate.method} ${candidate.url} ${JSON.stringify(candidate.body)}`.toLowerCase().includes(needle));
  if (!write) {
    if (!input.namedAction && input.actor === input.from && input.swap && input.match === input.swap.find) {
      const captured = capturedConvexMutation(source.page, transport.expand(input.swap.find), transport.expand(input.swap.with));
      if (captured) {
        const named = capabilities['named-actions'];
        const request = namedActionRequest(named, { id: captured.path, reducer: captured.path }, { values: captured.args });
        if (!request?.url || request.responseContract !== 'convex-mutation'
            || JSON.parse(request.body ?? '{}').path !== captured.path) {
          replayUnavailable(actor, 'the captured native mutation could not be bound to this deployment');
        }
        if (![new URL(request.url).origin, new URL(source.page.url()).origin].includes(captured.origin)) {
          replayUnavailable(actor, 'the captured mutation came from another origin');
        }
        const mine = await browserCredentials(actor, request.url);
        if (!mine) replayUnavailable(actor, 'the captured mutation has no current caller credentials');
        const bound = bindBrowserRequest(actor, { ...request, applicationOrigin: captured.origin }, mine)(
          { 'Content-Type': 'application/json', ...mine });
        try {
          const response = await named.fetch(request.url, { method: request.method ?? 'POST', ...bound, signal });
          const classified = classifyNamedActionResponse(named, request, { status: response.status, text: await response.text() });
          actor.replay = { ...classified, accepted: classified.ok, status: response.status,
            url: request.url, method: request.method ?? 'POST' };
        } catch (error) {
          if (harnessBrowserFailure(error)) throw error;
          actor.replay = { accepted: false, status: 0, complete: false };
        }
        await transport.sleep(input.settleMs ?? 2000, signal);
        return { attempted: true, accepted: actor.replay.accepted, status: actor.replay.status, capturedNativeMutation: true };
      }
    }
    if (input.namedAction) {
      const named = capabilities['named-actions'];
      const action = input.namedAction;
      const args = [...(action.args ?? [])];
      if (input.namedTarget) {
        const target = source.loc(input.namedTarget.testid, { contains: input.namedTarget.contains === undefined
          ? undefined : transport.expand(input.namedTarget.contains) });
        await target.waitFor({ state: 'visible', timeout: transport.defaultWithin });
        const rawValue = await target.getAttribute(input.namedTarget.attribute);
        if (rawValue === null || rawValue === '') {
          fail('interface-missing', { control: input.namedTarget.testid,
            attribute: input.namedTarget.attribute, action: action.id ?? 'replay' });
        }
        let value: string | number = rawValue;
        if (input.swap) {
          value = value.replaceAll(transport.expand(input.swap.find),
            transport.expand(input.swap.with));
        }
        if (input.namedTarget.valueType === 'number') {
          value = Number(value);
          if (!Number.isSafeInteger(value)) {
            fail('interface-invalid', { action: action.id ?? 'replay',
              attribute: input.namedTarget.attribute, detail: 'not a safe integer' });
          }
        }
        args[0] = value;
      } else if (input.swap) {
        const find = transport.expand(input.swap.find);
        const replacement = transport.expand(input.swap.with);
        const index = args.findIndex(value => typeof value === 'string' && value.includes(find));
        if (index < 0) {
          replayUnavailable(actor,
            `literal "${find}" does not appear in the arguments for named action "${action.id}"`);
        }
        args[index] = String(args[index]).replaceAll(find, replacement);
      }
      const request = namedActionRequest(named, action, { ...input, args });
      if (!request?.url) {
        replayUnavailable(actor,
          `could not resolve where to send named action "${action.id}" for this backend`);
      }
      const mine = await browserCredentials(actor, request.url);
      if (!mine) {
        replayUnavailable(actor,
          `no credentials found for ${actor.name} — an anonymous replay only shows that unauthenticated requests are refused`);
      }
      const bound = bindBrowserRequest(actor, request, mine)({ 'Content-Type': 'application/json', ...mine });
      try {
        const response = await named.fetch(request.url, {
          method: request.method ?? 'POST', ...bound, signal,
        });
        const classified = classifyNamedActionResponse(named, request, { status: response.status, text: await response.text() });
        actor.replay = { ...classified, accepted: classified.ok, status: response.status, url: request.url,
          method: request.method ?? 'POST', namedAction: action.id };
      } catch { actor.replay = { accepted: false, status: 0, complete: false, namedAction: action.id }; }
      await transport.sleep(input.settleMs ?? 2000, signal);
      return { attempted: true, accepted: actor.replay.accepted, status: actor.replay.status,
        namedAction: action.id };
    }
    replayUnavailable(actor, source.lastWsWrite
      ? `${input.from} writes over WebSocket ("${source.lastWsWrite.event}") — identity comes from the connection, replay not attempted`
      : `no HTTP write from ${input.from} matching "${input.match}"`);
  }
  let url = write.url;
  let data = write.body === null ? undefined : JSON.stringify(write.body);
  if (input.swap) {
    const find = transport.expand(input.swap.find);
    const to = transport.expand(input.swap.with);
    let fromToken = find;
    let toToken = to;
    if (!mentions(url, fromToken) && !mentions(data ?? '', fromToken)) {
      if (numericLiteral(find) || numericLiteral(to)) {
        replayUnavailable(actor,
          `literal "${find}" does not appear in ${write.method} ${write.url} — the request has no value to edit`);
      }
      const candidates = [...discoverIds(source, find), ...discoverIds(actor, find)];
      const resolvedFrom = candidates.find(candidate =>
        mentions(url, candidate.value) || mentions(data ?? '', candidate.value));
      const targets = [...discoverIds(source, to), ...discoverIds(actor, to)];
      const matchingValues = new Set(targets.filter(candidate => candidate.value !== resolvedFrom?.value
          && candidate.key === resolvedFrom?.key
          && candidate.relationDepth === resolvedFrom?.relationDepth)
        .map(candidate => candidate.value));
      const resolvedTo = matchingValues.values().next();
      if (!resolvedFrom || matchingValues.size !== 1 || resolvedTo.done) {
        replayUnavailable(actor,
          `could not resolve one matching id from "${find}" to "${to}" in ${write.method} ${write.url} — cannot retarget the replay safely`);
      }
      fromToken = resolvedFrom.value;
      toToken = resolvedTo.value;
    }
    url = swapToken(url, fromToken, toToken);
    if (data) data = swapToken(data, fromToken, toToken);
  }
  if (new URL(url).origin !== new URL(write.url).origin) {
    replayUnavailable(actor, 'retargeting a replay cannot change its origin');
  }
  const mine = await browserCredentials(actor, url);
  if (!mine) {
    replayUnavailable(actor,
      `no credentials found for ${actor.name} — an anonymous replay only shows that unauthenticated requests are refused`);
  }
  const credentials = { ...mine };
  for (const key of Object.keys(write.headers)) {
    if (REQUEST_CONTEXT_HEADER.test(key)
      && !Object.keys(credentials).some(candidate => candidate.toLowerCase() === key.toLowerCase())) {
      if (/csrf|xsrf/i.test(key)) {
        replayUnavailable(actor, `no caller ${key} context; a CSRF rejection would not establish authorization`);
      }
      credentials[key] = '';
    }
  }
  const responseContract = classifyNamedActionResponse(capabilities['named-actions'] ?? {}, { url, method: write.method },
    { status: 0, text: '' }).responseContract;
  const bound = bindBrowserRequest(actor, { url, body: data, responseContract }, mine)(replayHeaders(write, credentials));
  const response = await actor.page.request.fetch(url, {
    method: write.method,
    headers: bound.headers,
    ...(bound.body === undefined ? {} : { data: bound.body }),
  }).catch(error => {
    if (harnessBrowserFailure(error)) throw error;
    return { status: () => 0, ok: () => false, error: error.message };
  });
  try {
    const classified = classifyNamedActionResponse(capabilities['named-actions'] ?? {}, { url, method: write.method },
      { status: response.status(), text: 'text' in response && response.text ? await response.text() : '' });
    actor.replay = { ...classified, accepted: classified.ok, status: response.status(), url, method: write.method };
  } catch { actor.replay = { accepted: false, status: 0, complete: false, url, method: write.method }; }
  await transport.sleep(input.settleMs ?? 2000, signal);
  return { attempted: true, accepted: actor.replay.accepted, status: actor.replay.status };
}

// This checks delivery only. The scenario must separately prove a single effect
// from fresh application state; authorization replays use expectReplayRejected.
async function expectReplayCompleted({ input, capabilities }: TransportArguments<ActorInput & { requireAccepted?: boolean }>) {
  const actor = actorFor(capabilities, input.actor);
  const replay = actor.replay;
  if (!replay) inconclusive('assertion-without-action', { action: 'replayAs' });
  if (replay.inconclusive) {
    inconclusive('replay-unavailable', { actor: actor.name, detail: replay.reason ?? '' });
  }
  if (replay.complete === false) inconclusive('transport-incomplete', {});
  if (input.requireAccepted && !replay.accepted) {
    fail('call-error', { action: replay.namedAction ?? 'replay', actor: actor.name,
      status: replay.status ?? null, required: 'accepted', operation: null });
  }
  if (!replay.accepted && !(!replay.responseContract?.startsWith('convex-') && [400, 409, 422].includes(replay.status ?? 0))
    && replay.applicationRejected !== true && replay.refusalKind !== 'validation') {
    fail('call-error', { action: replay.namedAction ?? 'replay', actor: actor.name,
      status: replay.status ?? null, required: 'validation-refused', operation: null });
  }
  return { status: replay.status, accepted: replay.accepted };
}

async function expectReplayRejected({ input, capabilities }:
    TransportArguments<ActorInput & { allowNotFound?: boolean }>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const replay = actor.replay;
  if (!replay) inconclusive('assertion-without-action', { action: 'replayAs' });
  if (replay.inconclusive) {
    transport.verification.unverified(`${actor.name}: ${replay.reason}`);
    inconclusive('replay-unavailable', { actor: actor.name, detail: replay.reason ?? '' });
  }
  if (replay.complete === false) inconclusive('transport-incomplete', {});
  const named = replay.namedAction ? { action: replay.namedAction } : {};
  if (replay.accepted) {
    fail('replay-accepted', { actor: actor.name, status: replay.status ?? null, ...named });
  }
  const replayStatus = replay.status;
  const httpRefusal = !replay.responseContract?.startsWith('convex-')
    && (replayStatus === 401 || replayStatus === 403 || (replayStatus === 404 && input.allowNotFound === true));
  if (!httpRefusal && replay.refusalKind !== 'access' && replay.applicationRejected !== true) {
    fail('replay-error', { status: replay.status ?? null, ...named });
  }
  transport.verification.verified(
    `${actor.name}: server refused ${replay.method} ${replay.url} (HTTP ${replay.status})`);
  return { classification: 'verified', status: replay.status };
}

async function expectReceived({ input, capabilities, signal }: TransportArguments<ReceiveInput>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const needle = transport.expand(input.contains);
  const deadline = Date.now() + (input.within ?? transport.defaultWithin);
  while (!actor.wasSent(needle, false) && Date.now() < deadline) await transport.sleep(250, signal);
  if (!actor.wasSent(needle, false)) inconclusive('not-observed', { actor: actor.name });
  return { received: true, contains: needle };
}

async function expectNotReceived({ input, capabilities, signal }: TransportArguments<ReceiveInput>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const needle = transport.expand(input.contains);
  await transport.sleep(input.within ?? transport.defaultWithin, signal);
  if (actor.wasSent(needle)) fail('message-delivered', { actor: actor.name });
  return { received: false, contains: needle };
}

export const ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS = Object.freeze({
  ...CHAT_ACTION_IMPLEMENTATIONS,
  ...NAMED_ACTION_IMPLEMENTATIONS,
  expectForgeryRejected: actionImplementation(expectForgeryRejected),
  expectNotReceived: actionImplementation(expectNotReceived),
  expectReceived: actionImplementation(browserApplicationBoundary(expectReceived)),
  expectReplayCompleted: actionImplementation(expectReplayCompleted),
  expectReplayRejected: actionImplementation(expectReplayRejected),
  forgeWrite: actionImplementation(browserApplicationBoundary(forgeWrite)),
  replayAs: actionImplementation(browserApplicationBoundary(replayAs)),
  repeatFormWrite: actionImplementation(browserApplicationBoundary(repeatFormWrite)),
  runScript: RUN_SCRIPT_ACTION_IMPLEMENTATION,
});
