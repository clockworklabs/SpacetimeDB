import { actionImplementation } from './action-contract.js';
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
import { browserApplicationBoundary } from './browser-action-executors.js';
import { harnessBrowserFailure } from '../evidence/harness-errors.js';
import { createParser } from 'eventsource-parser';
import { RUN_SCRIPT_ACTION_IMPLEMENTATION } from './application-process-action-executors.js';
import { CHAT_ACTION_IMPLEMENTATIONS } from './chat-action-executors.js';
import { NAMED_ACTION_IMPLEMENTATIONS } from './named-action-executors.js';
import {
  browserCredentials,
  capturedCredentials,
  namedActionRequest,
} from './named-action-runtime.js';
import type { NamedAction, NamedActionsCapability } from './named-action-runtime.js';

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

type TransportCapabilities = TransportActorCapabilities;

interface ReplayCapabilities extends TransportCapabilities {
  readonly 'named-actions': NamedActionsCapability;
}

type TransportArguments<Input extends ActorInput> =
  ActorActionArguments<Input, TransportCapabilities>;
type ReplayArguments = ActorActionArguments<ReplayInput, ReplayCapabilities>;

const IDENTITY_FIELD = /^(user_?id|sender_?id|author_?id|from_?user|identity)$/i;
const CONTENT_FIELD = /^(content|text|message|body|msg)$/i;
const ROOM_FIELD = /^(room_?id|channel_?id|conversation_?id)$/i;
const AUTH_HEADER = /^(authorization|cookie|x-auth-token|x-session|x-token|x-user)$/i;
const ID_FIELD = /^(?:_?id|[A-Za-z][A-Za-z0-9_]*_?id)$/i;
const ID_KEY = /"(_?id|[A-Za-z][A-Za-z0-9_]*_?id)"\s*:\s*"?([A-Za-z0-9_-]{1,64})"?/gi;

function replayUnavailable(actor: Actor, reason: string): never {
  actor.replay = { inconclusive: true, reason };
  inconclusive(`could not issue the server-side replay: ${reason}`);
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
  const write = actor.lastWrite;
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
  const body = { ...write.body };
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
    const victimKey = Object.keys(victim.lastWrite?.body ?? {}).find(field => targetField.test(field));
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
      value = victim.lastWrite?.body?.[victimKey];
    }
  }
  body[key] = value;
  const contentKey = Object.keys(body).find(field => CONTENT_FIELD.test(field));
  if (contentKey && input.text) body[contentKey] = input.text;
  const response = await actor.page.request.fetch(write.url, {
    method: write.method,
    headers: replayHeaders(write),
    data: JSON.stringify(body),
  });
  const forgeResult: ForgeResult = {
    status: response.status(),
    accepted: response.ok(),
    tamperedField: key,
    reason: 'tampered request sent',
  };
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
  if (!forge) fail('no forgeWrite ran before this assertion');
  if (forge.inconclusive) {
    transport.verification.unverified(`${actor.name}: ${forge.reason}`);
    inconclusive(`could not verify the server-side forgery refusal: ${forge.reason}`);
  }
  if (forge.accepted) {
    fail(`server ACCEPTED a write with a tampered "${forge.tamperedField}" (HTTP ${forge.status}) — the client chooses who it is`);
  }
  if (forge.status !== 401 && forge.status !== 403) {
    fail(`the forged request failed with ${forge.status ? `HTTP ${forge.status}` : 'no server response'} — this does not prove an authorization refusal`);
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
    .find(candidate => `${candidate.method} ${candidate.url} ${JSON.stringify(candidate.body)}`
      .toLowerCase().includes(needle));
  if (!write) {
    if (input.namedAction && input.namedTarget) {
      const named = capabilities['named-actions'];
      const action = input.namedAction;
      const target = source.loc(input.namedTarget.testid, { contains: input.namedTarget.contains });
      await target.waitFor({ state: 'visible', timeout: transport.defaultWithin });
      const rawValue = await target.getAttribute(input.namedTarget.attribute);
      if (rawValue === null || rawValue === '') {
        replayUnavailable(actor,
          `${input.namedTarget.testid} exposes no ${input.namedTarget.attribute} value for the named replay`);
      }
      let value: string | number = rawValue;
      if (input.swap) {
        value = value.replaceAll(transport.expand(input.swap.find),
          transport.expand(input.swap.with));
      }
      if (input.namedTarget.valueType === 'number') {
        value = Number(value);
        if (!Number.isSafeInteger(value)) {
          replayUnavailable(actor,
            `${input.namedTarget.attribute} is not a safe integer for named action "${action.id}"`);
        }
      }
      const mine = capturedCredentials(actor) ?? await browserCredentials(actor);
      if (!mine) {
        replayUnavailable(actor,
          `no credentials found for ${actor.name} — an anonymous replay only shows that unauthenticated requests are refused`);
      }
      const request = namedActionRequest(named, action,
        { ...input, args: [value, ...(action.args ?? []).slice(1)] });
      if (!request?.url) {
        replayUnavailable(actor,
          `could not resolve where to send named action "${action.id}" for this backend`);
      }
      const response = await named.fetch(request.url, {
        method: request.method ?? 'POST',
        headers: { 'Content-Type': 'application/json', ...mine },
        body: request.body,
        signal,
      }).catch(error => ({ status: 0, ok: false, error: error.message }));
      actor.replay = { accepted: response.ok, status: response.status, url: request.url,
        method: request.method ?? 'POST', namedAction: action.id,
        applicationRejected: (request.applicationRejectionStatuses ?? []).includes(response.status) };
      await transport.sleep(input.settleMs ?? 2000, signal);
      return { attempted: true, accepted: actor.replay.accepted, status: actor.replay.status,
        namedAction: action.id };
    }
    replayUnavailable(actor, source.lastWsWrite
      ? `${input.from} writes over WebSocket ("${source.lastWsWrite.event}") — identity comes from the connection, replay not attempted`
      : `no HTTP write from ${input.from} matching "${input.match}"`);
  }
  const mine = capturedCredentials(actor) ?? await browserCredentials(actor);
  if (!mine) {
    replayUnavailable(actor,
      `no credentials found for ${actor.name} — an anonymous replay only shows that unauthenticated requests are refused`);
  }
  const credentials = { ...mine };
  for (const key of Object.keys(write.headers)) {
    if (AUTH_HEADER.test(key)
      && !Object.keys(credentials).some(candidate => candidate.toLowerCase() === key.toLowerCase())) {
      credentials[key] = '';
    }
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
  const response = await actor.page.request.fetch(url, {
    method: write.method,
    headers: replayHeaders(write, credentials),
    ...(data === undefined ? {} : { data }),
  }).catch(error => {
    if (harnessBrowserFailure(error)) throw error;
    return { status: () => 0, ok: () => false, error: error.message };
  });
  actor.replay = { accepted: response.ok(), status: response.status(), url, method: write.method };
  await transport.sleep(input.settleMs ?? 2000, signal);
  return { attempted: true, accepted: actor.replay.accepted, status: actor.replay.status };
}

async function expectReplayRejected({ input, capabilities }:
    TransportArguments<ActorInput & { allowNotFound?: boolean }>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const replay = actor.replay;
  if (!replay) fail('no replayAs ran before this assertion');
  if (replay.inconclusive) {
    transport.verification.unverified(`${actor.name}: ${replay.reason}`);
    inconclusive(`could not verify the server-side replay refusal: ${replay.reason}`);
  }
  if (replay.accepted) {
    fail(`server ACCEPTED ${replay.method} ${replay.url} from ${actor.name}, who is not allowed to do it (HTTP ${replay.status}) — the check is in the interface, not the server`);
  }
  const replayStatus = replay.status;
  if (replayStatus !== 401 && replayStatus !== 403
    && !(replayStatus === 404 && input.allowNotFound === true)
    && replay.applicationRejected !== true) {
    fail(`the ${replay.namedAction ? `named action "${replay.namedAction}"` : 'replayed request'} failed with `
      + `${replay.status ? `HTTP ${replay.status}` : 'no server response'} — this does not prove an authorization refusal`);
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
  while (!actor.wasSent(needle) && Date.now() < deadline) await transport.sleep(250, signal);
  if (!actor.wasSent(needle)) {
    fail(`the harness never saw "${needle}" reach ${actor.name}, who should have it — traffic on this app is not visible to the wire checks`);
  }
  return { received: true, contains: needle };
}

async function expectNotReceived({ input, capabilities, signal }: TransportArguments<ReceiveInput>) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const needle = transport.expand(input.contains);
  await transport.sleep(input.within ?? transport.defaultWithin, signal);
  if (actor.wasSent(needle)) {
    fail(`"${needle}" was delivered to ${actor.name}, who is not a participant — the server sends private data to everyone and relies on the client to hide it`);
  }
  return { received: false, contains: needle };
}

export const ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS = Object.freeze({
  ...CHAT_ACTION_IMPLEMENTATIONS,
  ...NAMED_ACTION_IMPLEMENTATIONS,
  expectForgeryRejected: actionImplementation(expectForgeryRejected),
  expectNotReceived: actionImplementation(expectNotReceived),
  expectReceived: actionImplementation(browserApplicationBoundary(expectReceived)),
  expectReplayRejected: actionImplementation(expectReplayRejected),
  forgeWrite: actionImplementation(browserApplicationBoundary(forgeWrite)),
  replayAs: actionImplementation(browserApplicationBoundary(replayAs)),
  runScript: RUN_SCRIPT_ACTION_IMPLEMENTATION,
});
