import { execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';
import { isAbsolute, relative, resolve } from 'node:path';
import {
  ActionApplicationFailure,
  ActionInconclusive,
} from './action-contract.js';
import type {
  ActionImplementation,
  ActionImplementationArguments,
} from './action-contract.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import { executeStackCapability } from '../stacks/stack-adapter-contract.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.mjs';
import { harnessProcessFailure } from '../evidence/harness-errors.js';

type UnknownRecord = Record<string, unknown>;
type HeaderRecord = Record<string, string>;

interface Locator {
  click(options?: unknown): Promise<void>;
  count(): Promise<number>;
  fill(value: string): Promise<void>;
  first(): Locator;
  getAttribute(name: string): Promise<string | null>;
  isVisible(): Promise<boolean>;
  press(key: string): Promise<void>;
  waitFor(options?: unknown): Promise<void>;
}

interface BrowserResponse {
  ok(): boolean;
  status(): number;
}

interface BrowserPage {
  readonly request: {
    fetch(url: string, options: UnknownRecord): Promise<BrowserResponse>;
  };
  evaluate<Result>(callback: () => Result): Promise<Result>;
  getByRole(role: string, options: UnknownRecord): Locator;
  locator(selector: string, options?: UnknownRecord): Locator;
}

interface CapturedWrite {
  readonly body: UnknownRecord | null;
  readonly headers: HeaderRecord;
  readonly method: string;
  readonly url: string;
}

interface ReplayResult {
  readonly accepted?: boolean;
  readonly applicationRejected?: boolean;
  readonly inconclusive?: boolean;
  readonly method?: string;
  readonly namedAction?: string;
  readonly reason?: string;
  readonly status?: number;
  readonly url?: string;
}

interface ForgeResult {
  readonly accepted?: boolean;
  readonly inconclusive?: boolean;
  readonly reason: string;
  readonly skipped?: boolean;
  readonly status?: number;
  readonly structurallySafe?: boolean;
  readonly tamperedField?: string;
  readonly value?: unknown;
}

interface ActionCall {
  readonly accepted: boolean;
  readonly action: string;
  readonly applicationRejected: boolean;
  readonly method: string;
  readonly operation: {
    readonly reducer: string | null;
    readonly path: string | null;
    readonly method: string;
  };
  readonly status: number;
  readonly url: string;
}

interface Actor {
  readonly context: {
    cookies(): Promise<readonly { readonly name: string; readonly value: string }[]>;
  };
  readonly lastWrite?: CapturedWrite;
  readonly lastWsWrite?: { readonly event: string; readonly body: UnknownRecord };
  readonly name: string;
  readonly page: BrowserPage;
  readonly received: readonly string[];
  readonly writes: readonly CapturedWrite[];
  actionCall?: ActionCall;
  forge?: ForgeResult;
  replay?: ReplayResult;
  loc(testid: string, options?: UnknownRecord): Locator;
  wasSent(value: string): boolean;
}

interface BrowserCapability {
  readonly defaultWithin: number;
  hyphenatedScopedUser(value: string): string;
  roomName(value: string): string;
  scopedUser(value: string): string;
  sleep(milliseconds: number, signal: AbortSignal): Promise<void>;
  testId(id: string): string;
}

interface TransportCapability {
  readonly defaultWithin: number;
  readonly verification: {
    structural(message: string): void;
    unverified(message: string): void;
    verified(message: string): void;
  };
  expand(value: string): string;
  sleep(milliseconds: number, signal: AbortSignal): Promise<void>;
}

interface NamedActionParameter {
  readonly in?: 'body' | 'path';
  readonly name: string;
  readonly placeholder?: string;
  readonly wireType?: string;
}

interface NamedAction {
  readonly args?: readonly unknown[];
  readonly id: string;
  readonly method?: string;
  readonly params?: readonly NamedActionParameter[];
  readonly path?: string;
  readonly reducer?: string;
}

interface NamedActionRequest {
  readonly applicationRejectionStatuses?: readonly number[];
  readonly body?: string | null;
  readonly method?: string;
  readonly url?: string | null;
}

interface ConcurrentCallOutcome {
  readonly name: string;
  readonly ok: boolean;
  readonly status: number;
  readonly text: string;
}

interface ConcurrentCallResult {
  readonly action: string;
  readonly fired: number;
  readonly ms: number;
  readonly outcomes: readonly ConcurrentCallOutcome[];
}

interface NamedActionsCapability {
  readonly lastCalls: {
    get(): ConcurrentCallResult | null;
    set(result: ConcurrentCallResult): void;
  };
  fetch(url: string, options: {
    readonly body?: string | null;
    readonly headers?: HeaderRecord;
    readonly method?: string;
    readonly signal?: AbortSignal;
  }): Promise<{
    readonly ok: boolean;
    readonly status: number;
    text(): Promise<string>;
  }>;
  now(): number;
  request(action: NamedAction, input: unknown): NamedActionRequest | null;
  resolve(id: string): NamedAction | null;
  sleep(milliseconds: number, signal: AbortSignal): Promise<void>;
}

interface CompiledActorTransportInput {
  readonly accepted?: number;
  readonly action: string;
  readonly actor: string;
  readonly actors: readonly string[];
  readonly args?: readonly string[];
  readonly authentication?: 'actor' | 'anonymous';
  readonly contains: string;
  readonly count: number;
  readonly delayMs?: number;
  readonly exact?: boolean;
  readonly expectFailure?: boolean;
  readonly field: 'room' | 'identity';
  readonly from: string;
  readonly fromActor?: string;
  readonly input: {
    readonly attribute: string;
    readonly contains?: string;
    readonly testid: string;
  };
  readonly match: string;
  readonly name: string;
  readonly namedAction?: NamedAction;
  readonly namedTarget?: {
    readonly attribute: string;
    readonly contains?: string;
    readonly testid: string;
    readonly valueType?: 'number' | 'string';
  };
  readonly outcome: 'accepted' | 'rejected';
  readonly password?: string;
  readonly prefix: string;
  readonly private?: boolean;
  readonly readyTestid?: string;
  readonly room: string;
  readonly routeProvenBy?: string;
  readonly script: string;
  readonly secondsAhead: number;
  readonly settleMs?: number;
  readonly swap?: { readonly find: string; readonly with: string };
  readonly text: string;
  readonly timeoutMs?: number;
  readonly value: unknown;
  readonly within?: number;
}

interface ActorTransportCapabilities {
  readonly actors: { get(name: string): Actor | undefined };
  readonly 'application-files': {
    readonly root?: string;
    expand(value: string): string;
  };
  readonly 'browser-interaction': BrowserCapability;
  readonly 'named-actions': NamedActionsCapability;
  readonly subprocess: { sleep(milliseconds: number, signal: AbortSignal): Promise<void> };
  readonly 'transport-observation': TransportCapability;
}

interface ActorArguments {
  readonly capabilities: ActorTransportCapabilities;
  readonly input: CompiledActorTransportInput;
  readonly signal: AbortSignal;
}

interface StorageLike {
  readonly length: number;
  getItem(key: string): string | null;
  key(index: number): string | null;
}

declare const localStorage: StorageLike;
declare const sessionStorage: StorageLike;

export const ACTOR_TRANSPORT_ACTION_IDS = Object.freeze([
  'callAction',
  'callConcurrently',
  'createRoom',
  'ensureRegistered',
  'ensureSignedIn',
  'enterRoom',
  'expectCallOutcomes',
  'expectActionOutcome',
  'expectForgeryRejected',
  'expectNotReceived',
  'expectReceived',
  'expectReplayRejected',
  'forgeWrite',
  'register',
  'replayAs',
  'runScript',
  'scheduleMessage',
  'send',
  'sendMany',
  'signIn',
  'signUp',
].sort());

const IDENTITY_FIELD = /^(user_?id|sender_?id|author_?id|from_?user|identity)$/i;
const CONTENT_FIELD = /^(content|text|message|body|msg)$/i;
const ROOM_FIELD = /^(room_?id|channel_?id|conversation_?id)$/i;
const AUTH_HEADER = /^(authorization|cookie|x-auth-token|x-session|x-token|x-user)/i;
const ID_FIELD = /^(?:_?id|[A-Za-z][A-Za-z0-9_]*_?id)$/i;
const ID_KEY = /"(_?id|[A-Za-z][A-Za-z0-9_]*_?id)"\s*:\s*"?([A-Za-z0-9_-]{1,64})"?/gi;

function fail(message: string): never {
  throw new ActionApplicationFailure(message);
}

function inconclusive(message: string): never {
  throw new ActionInconclusive(message);
}

function replayUnavailable(actor: Actor, reason: string): never {
  actor.replay = { inconclusive: true, reason };
  inconclusive(`could not issue the server-side replay: ${reason}`);
}
const pad = (index: number, count: number): string =>
  String(index).padStart(String(count).length, '0');

function actorFor(capabilities: ActorTransportCapabilities, name: string): Actor {
  const actor = capabilities.actors.get(name);
  if (!actor) throw new ActionApplicationFailure(`unknown actor "${name}"`);
  return actor;
}

const browserFor = (capabilities: ActorTransportCapabilities): BrowserCapability =>
  capabilities['browser-interaction'];
const transportFor = (capabilities: ActorTransportCapabilities): TransportCapability =>
  capabilities['transport-observation'];

function errorField(error: unknown, field: string): unknown {
  return typeof error === 'object' && error !== null
    ? (error as Record<string, unknown>)[field]
    : undefined;
}

function namedActionRequest(
  named: NamedActionsCapability,
  action: NamedAction,
  input: unknown,
): NamedActionRequest | null {
  try { return named.request(action, input); }
  catch (error) {
    if (errorField(error, 'code') === 'invalid_named_action_input') {
      fail(String(errorField(error, 'message') ?? 'invalid named action input'));
    }
    throw error;
  }
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
    const sse = line.match(/^data:\s*(.+)$/);
    if (sse?.[1]) candidates.add(sse[1]);
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

// Return ids from the entity containing `needle`, deepest first. Keeping the
// id field and its distance from the matching value lets a replay map an order
// id to another order id instead of accidentally substituting a nested item id.
// JSON is preferred; the text fallback covers NDJSON/SSE and opaque payloads.
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
  const headers: HeaderRecord = {
    ...(write.headers ?? {}),
    'content-type': 'application/json',
    ...overrides,
  };
  for (const key of Object.keys(headers)) {
    if (/^(content-length|host|connection|transfer-encoding|accept-encoding)$/i.test(key)) {
      delete headers[key];
    }
  }
  return headers;
}

function authFor(actor: Actor): HeaderRecord | null {
  for (const write of [...actor.writes].reverse()) {
    const headers = Object.entries(write.headers ?? {}).filter(([key, value]) => AUTH_HEADER.test(key) && value);
    if (headers.length) return Object.fromEntries(headers);
  }
  for (const chunk of actor.received) {
    const match = chunk.match(/"(?:token|accessToken|access_token|jwt|sessionToken)"\s*:\s*"([^"]{8,})"/);
    if (match) return { authorization: `Bearer ${match[1]}` };
  }
  return null;
}

async function browserCredentials(actor: Actor): Promise<HeaderRecord | null> {
  const headers: HeaderRecord = {};
  const cookies = await actor.context.cookies().catch(() => []);
  if (cookies.length) headers.Cookie = cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ');
  const token = await actor.page.evaluate(() => {
    const jwt = /^eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\./;
    const credentialKey = /(?:^|[_-])(auth|jwt|session|token)(?:$|[_-])|(?:auth|jwt|session|token)$/i;
    const usable = (value: unknown): value is string =>
      typeof value === 'string' && value.trim().length >= 8;
    const fromObject = (value: unknown): string | null => {
      if (!value || typeof value !== 'object') return null;
      for (const [key, nested] of Object.entries(value)) {
        if (credentialKey.test(key) && usable(nested)) return nested.trim();
      }
      return null;
    };
    for (const storage of [localStorage, sessionStorage]) {
      for (let index = 0; index < storage.length; index++) {
        const key = storage.key(index) ?? '';
        const value = storage.getItem(key) ?? '';
        if (jwt.test(value) || (credentialKey.test(key) && usable(value))) return value.trim();
        try {
          const object = JSON.parse(value);
          const nested = fromObject(object);
          if (nested) return nested;
        } catch { /* not JSON */ }
      }
    }
    return null;
  }).catch(() => null);
  if (token) headers.Authorization = `Bearer ${token}`;
  return Object.keys(headers).length ? headers : null;
}

async function signUp({ input, capabilities, signal }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const user = input.exact ? input.name : browser.scopedUser(input.name);
  const password = input.password ?? `pw-${user}`;
  await actor.page.locator(browser.testId('signup-username')).first().fill(user);
  await actor.page.locator(browser.testId('signup-password')).first().fill(password);
  await actor.page.locator(browser.testId('signup-submit')).first().click();
  if (input.expectFailure) {
    await browser.sleep(input.settleMs ?? 2000, signal);
    return { user, expectedFailure: true };
  }
  await actor.page.locator(browser.testId('current-user')).first()
    .waitFor({ state: 'visible', timeout: browser.defaultWithin * 2 });
  return { user, signedUp: true };
}

async function signIn({ input, capabilities, signal }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const user = input.exact ? input.name : browser.scopedUser(input.name);
  const password = input.password ?? `pw-${user}`;
  const username = actor.page.locator(browser.testId('signin-username')).first();
  const toggle = actor.loc('signin-toggle');
  if (!(await username.isVisible())) {
    await toggle.waitFor({ state: 'visible', timeout: browser.defaultWithin });
    await toggle.click({ timeout: browser.defaultWithin });
    await username.waitFor({ state: 'visible', timeout: browser.defaultWithin });
  }
  await username.fill(user);
  await actor.page.locator(browser.testId('signin-password')).first().fill(password);
  await actor.page.locator(browser.testId('signin-submit')).first().click();
  if (input.expectFailure) {
    await browser.sleep(input.settleMs ?? 2000, signal);
    return { user, expectedFailure: true };
  }
  await actor.page.locator(browser.testId('current-user')).first()
    .waitFor({ state: 'visible', timeout: browser.defaultWithin * 2 });
  return { user, signedIn: true };
}

async function register({ input, capabilities }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const name = browser.hyphenatedScopedUser(input.name);
  await actor.page.locator(browser.testId('name-input')).first().fill(name);
  await actor.page.locator(browser.testId('name-submit')).first().click();
  await actor.page.locator(browser.testId('room-list')).first()
    .waitFor({ state: 'attached', timeout: browser.defaultWithin });
  return { name, registered: true };
}

async function createRoom({ input, capabilities }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const room = browser.roomName(input.room);
  const nameInput = actor.page.locator(browser.testId('room-name-input')).first();
  if (!(await nameInput.isVisible().catch(() => false))) {
    await actor.page.locator(browser.testId('room-create')).first().click();
  }
  await nameInput.fill(room);
  if (input.private) await actor.page.locator(browser.testId('room-private-toggle')).first().click();
  await actor.page.locator(browser.testId('room-name-submit')).first().click();
  await actor.page.locator(browser.testId('room-item'), { hasText: room }).first()
    .waitFor({ state: 'visible', timeout: browser.defaultWithin });
  return { room, private: input.private === true };
}

async function enterRoom({ input, capabilities, signal }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const room = browser.roomName(input.room);
  const item = actor.page.locator(browser.testId('room-item'), { hasText: room }).first();
  await item.waitFor({ state: 'visible', timeout: browser.defaultWithin });
  await item.click();
  const message = actor.loc('message-input');
  if (!(await message.isVisible().catch(() => false))) {
    await browser.sleep(750, signal);
    if (!(await message.isVisible().catch(() => false))) await item.click();
  }
  await message.waitFor({ state: 'visible', timeout: browser.defaultWithin });
  return { room, entered: true };
}

async function send({ input, capabilities }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const message = actor.loc('message-input');
  await message.fill(input.text);
  await message.press('Enter');
  return { sent: input.text };
}

async function sendMany({ input, capabilities, signal }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const message = actor.loc('message-input');
  for (let index = 1; index <= input.count; index++) {
    await message.fill(`${input.prefix}-${pad(index, input.count)}`);
    await message.press('Enter');
    if (input.delayMs) await browser.sleep(input.delayMs, signal);
  }
  return { sent: input.count, prefix: input.prefix };
}

async function scheduleMessage({ input, capabilities }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const message = actor.loc('message-input');
  if (await message.count()) await message.fill(input.text);
  const toggle = actor.loc('schedule-toggle');
  if (await toggle.count()) await toggle.click({ timeout: browser.defaultWithin }).catch(() => {});
  const when = actor.loc('schedule-time');
  await when.waitFor({ state: 'visible', timeout: browser.defaultWithin });
  const dedicated = actor.page.locator(
    '[data-testid="schedule-message-input"], #schedule-message-input, '
      + '[data-testid="schedule-text"], #schedule-text').first();
  if (await dedicated.count()) await dedicated.fill(input.text);
  const type = (await when.getAttribute('type')) ?? 'text';
  const at = new Date(Date.now() + input.secondsAhead * 1000);
  const local = new Date(at.getTime() - at.getTimezoneOffset() * 60000).toISOString();
  if (type === 'datetime-local') await when.fill(local.slice(0, 16));
  else if (type === 'time') await when.fill(local.slice(11, 16));
  else await when.fill(String(input.secondsAhead));
  const submit = actor.loc('schedule-submit');
  if (await submit.count()) await submit.click();
  else {
    const labelled = actor.page.getByRole('button', { name: /schedule|send later|confirm/i }).first();
    if (await labelled.count()) await labelled.click();
    else await when.press('Enter');
  }
  return { scheduled: input.text, secondsAhead: input.secondsAhead };
}

async function ensureSignedIn({ input, capabilities, signal }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const browser = browserFor(capabilities);
  const username = actor.page.locator(browser.testId('signup-username')).first();
  if (!(await username.isVisible().catch(() => false))) return { restored: false };
  const user = input.exact ? input.name : browser.scopedUser(input.name);
  await username.fill(user);
  await actor.page.locator(browser.testId('signup-password')).first().fill(input.password ?? `pw-${user}`);
  await actor.page.locator(browser.testId('signup-submit')).first().click();
  await actor.page.locator(browser.testId(input.readyTestid ?? 'room-list')).first()
    .waitFor({ state: 'attached', timeout: browser.defaultWithin });
  await browser.sleep(input.settleMs ?? 1500, signal);
  return { restored: true, user };
}

const ensureRegistered = ensureSignedIn;

async function forgeWrite({ input, capabilities, signal }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const write = actor.lastWrite;
  if (!write) {
    const websocket = actor.lastWsWrite;
    if (websocket) {
      const identityKey = Object.keys(websocket.body).find(key => IDENTITY_FIELD.test(key));
      actor.forge = identityKey
        ? { inconclusive: true, reason: `writes over WebSocket ("${websocket.event}") carrying a client-supplied "${identityKey}" — replay not attempted, treat as unverified` }
        : { structurallySafe: true, reason: `writes over WebSocket ("${websocket.event}") with no client-supplied identity — the server must derive it from the connection` };
    } else {
      actor.forge = { inconclusive: true, reason: 'no write request observed at all — the test did not exercise anything' };
    }
    return { attempted: false, classification: actor.forge.structurallySafe ? 'structural' : 'unverified' };
  }
  const body = { ...write.body };
  const key = Object.keys(body).find(field => (input.field === 'room' ? ROOM_FIELD : IDENTITY_FIELD).test(field));
  if (!key) {
    actor.forge = { structurallySafe: true,
      reason: `write body carries no identity field (${Object.keys(body).join(',') || 'empty'}) — nothing to forge` };
    return { attempted: false, classification: 'structural' };
  }
  let value = input.value;
  if (input.fromActor) {
    const victim = actorFor(capabilities, input.fromActor);
    const victimKey = Object.keys(victim.lastWrite?.body ?? {}).find(field => IDENTITY_FIELD.test(field));
    if (!victimKey) {
      const wsKey = victim.lastWsWrite && Object.keys(victim.lastWsWrite.body)
        .find(field => IDENTITY_FIELD.test(field));
      if (!wsKey) {
        actor.forge = { structurallySafe: true, reason: `${input.fromActor} never exposes an identity to steal` };
        return { attempted: false, classification: 'structural' };
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
    skipped: false,
    status: response.status(),
    accepted: response.ok(),
    tamperedField: key,
    value,
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

async function expectForgeryRejected({ input, capabilities }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const forge = actor.forge;
  if (!forge) fail('no forgeWrite ran before this assertion');
  if (forge.inconclusive) {
    transport.verification.unverified(`${actor.name}: ${forge.reason}`);
    inconclusive(`could not verify the server-side forgery refusal: ${forge.reason}`);
  }
  if (forge.structurallySafe) {
    transport.verification.structural(`${actor.name}: ${forge.reason}`);
    return { classification: 'structural' };
  }
  if (forge.accepted) {
    fail(`server ACCEPTED a write with a tampered "${forge.tamperedField}" (HTTP ${forge.status}) — the client chooses who it is`);
  }
  transport.verification.verified(
    `${actor.name}: server refused the tampered "${forge.tamperedField}" (HTTP ${forge.status})`);
  return { classification: 'verified', status: forge.status };
}

async function replayAs({ input, capabilities, signal }: ActorArguments) {
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
      const mine = authFor(actor) ?? await browserCredentials(actor);
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
  const mine = authFor(actor) ?? await browserCredentials(actor);
  if (!mine) {
    replayUnavailable(actor,
      `no credentials found for ${actor.name} — an anonymous replay only shows that unauthenticated requests are refused`);
  }
  const credentials = { ...mine };
  for (const key of Object.keys(write.headers)) {
    if (AUTH_HEADER.test(key) && !(key in credentials)) credentials[key] = '';
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
      const resolvedTo = targets.find(candidate => candidate.value !== resolvedFrom?.value
          && candidate.key === resolvedFrom?.key
          && candidate.relationDepth === resolvedFrom?.relationDepth)
        ?? targets.find(candidate => candidate.value !== resolvedFrom?.value
          && candidate.key === resolvedFrom?.key)
        ?? targets.find(candidate => candidate.value !== resolvedFrom?.value);
      if (!resolvedFrom || !resolvedTo) {
        replayUnavailable(actor,
          `neither "${find}" nor an id resolved for it appears in ${write.method} ${write.url} — cannot retarget the replay`);
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
  }).catch(error => ({ status: () => 0, ok: () => false, error: error.message }));
  actor.replay = { accepted: response.ok(), status: response.status(), url, method: write.method };
  await transport.sleep(input.settleMs ?? 2000, signal);
  return { attempted: true, accepted: actor.replay.accepted, status: actor.replay.status };
}

async function expectReplayRejected({ input, capabilities }: ActorArguments) {
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
  if (!(typeof replayStatus === 'number' && Number.isInteger(replayStatus)
      && replayStatus >= 400 && replayStatus < 500)
    && replay.applicationRejected !== true) {
    fail(`the ${replay.namedAction ? `named action "${replay.namedAction}"` : 'replayed request'} failed with `
      + `${replay.status ? `HTTP ${replay.status}` : 'no server response'} — this does not prove an authorization refusal`);
  }
  transport.verification.verified(
    `${actor.name}: server refused ${replay.method} ${replay.url} (HTTP ${replay.status})`);
  return { classification: 'verified', status: replay.status };
}

async function expectReceived({ input, capabilities, signal }: ActorArguments) {
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

async function expectNotReceived({ input, capabilities }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const needle = transportFor(capabilities).expand(input.contains);
  if (actor.wasSent(needle)) {
    fail(`"${needle}" was delivered to ${actor.name}, who is not a participant — the server sends private data to everyone and relies on the client to hide it`);
  }
  return { received: false, contains: needle };
}

async function callAction({ input, capabilities, signal }: ActorArguments) {
  const caller = actorFor(capabilities, input.actor);
  const source = actorFor(capabilities, input.from ?? input.actor);
  const named = capabilities['named-actions'];
  const transport = transportFor(capabilities);
  const action = input.namedAction ?? named.resolve(input.action);
  if (!action) inconclusive(`the track names no action "${input.action}", so nothing could be issued`);
  if (!Array.isArray(action.params) || action.params.length === 0) {
    inconclusive(`action "${input.action}" does not declare named input parameters`);
  }

  const target = source.loc(input.input.testid, { contains: input.input.contains });
  // The attribute is machine-readable action input. A drawer can cover its
  // source element without removing it from the application contract. Requiring
  // screen visibility here made layout decide a server-behaviour score.
  await target.waitFor({ state: 'attached', timeout: transport.defaultWithin });
  const raw = await target.getAttribute(input.input.attribute);
  if (raw === null || raw === '') {
    fail(`${input.input.testid} exposes no ${input.input.attribute} value for action "${input.action}"`);
  }
  let values;
  try { values = JSON.parse(raw); }
  catch { fail(`${input.input.attribute} must contain a JSON object for action "${input.action}"`); }
  if (!values || typeof values !== 'object' || Array.isArray(values)) {
    fail(`${input.input.attribute} must contain a JSON object for action "${input.action}"`);
  }
  const expected = action.params.map(param => param.name).sort();
  const actual = Object.keys(values).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    fail(`${input.input.attribute} for action "${input.action}" must contain exactly `
      + `${expected.join(', ') || '(no values)'}; found ${actual.join(', ') || '(none)'}`);
  }

  let credentials: HeaderRecord = {};
  if ((input.authentication ?? 'actor') === 'actor') {
    const actorCredentials = authFor(caller) ?? await browserCredentials(caller);
    if (!actorCredentials) {
      inconclusive(`no session found in ${caller.name}'s browser, so action "${input.action}" could not be issued as them`);
    }
    credentials = actorCredentials;
  }
  const request = namedActionRequest(named, action, { values });
  if (!request?.url) inconclusive(`could not resolve where to send action "${input.action}" for this backend`);
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
    // Retained so a 404 can name the operation the testing interface requires;
    // "not found" is otherwise indistinguishable from a wrongly-named endpoint
    // and a repair round has nothing to act on.
    operation: { reducer: action.reducer ?? null, path: action.path ?? null,
      method: action.method ?? 'POST' },
  };
  await transport.sleep(input.settleMs ?? 2000, signal);
  return { action: input.action, accepted: caller.actionCall.accepted,
    status: caller.actionCall.status };
}

// A 404 usually means the declared operation does not exist under its required
// name. Say what the testing interface calls, or the failure reads as an
// authorization verdict when it is a naming one and no repair can target it.
function missingOperationHint(call: ActionCall): string {
  if (call.status !== 404 || !call.operation) return '';
  const identities = [
    call.operation.reducer ? `SpacetimeDB reducer \`${call.operation.reducer}\`` : null,
    call.operation.path ? `\`${call.operation.method} ${call.operation.path}\`` : null,
  ].filter(Boolean).join(' or ');
  return identities
    ? ` — the public testing interface calls ${identities}, which must exist and apply the ordinary rules to this caller`
    : '';
}

async function expectActionOutcome({ input, capabilities }: ActorArguments) {
  const actor = actorFor(capabilities, input.actor);
  const transport = transportFor(capabilities);
  const call = actor.actionCall;
  if (!call) fail('no callAction ran before this assertion');
  if (input.outcome === 'accepted') {
    if (!call.accepted) {
      fail(`server did not accept action "${call.action}" as ${actor.name} `
        + `(${call.status ? `HTTP ${call.status}` : 'no server response'})`
        + missingOperationHint(call));
    }
  } else {
    if (call.accepted) {
      fail(`server accepted action "${call.action}" as ${actor.name}, who is not allowed to do it`);
    }
    const routeProof = input.routeProvenBy === undefined ? null
      : actorFor(capabilities, input.routeProvenBy).actionCall;
    const provenPrivateNotFound = call.status === 404 && routeProof?.accepted === true
      && routeProof.action === call.action;
    const deliberateRefusal = (Number.isInteger(call.status) && call.status >= 400
      && call.status < 500 && (call.status !== 404 || provenPrivateNotFound))
      || call.applicationRejected === true;
    if (!deliberateRefusal) {
      fail(`action "${call.action}" failed with ${call.status ? `HTTP ${call.status}` : 'no server response'}; `
        + 'that does not prove the server refused the caller'
        + missingOperationHint(call));
    }
  }
  transport.verification.verified(
    `${actor.name}: server ${input.outcome === 'accepted' ? 'accepted' : 'refused'} `
      + `action "${call.action}" (HTTP ${call.status})`);
  return { action: call.action, outcome: input.outcome, status: call.status,
    classification: 'verified' };
}

async function callConcurrently({ input, capabilities, signal }: ActorArguments) {
  const named = capabilities['named-actions'];
  const action = named.resolve(input.action);
  if (!action) inconclusive(`the track names no action "${input.action}", so nothing could be issued`);
  const prepared: Array<{ name: string; credentials: HeaderRecord }> = [];
  for (const name of input.actors) {
    const actor = actorFor(capabilities, name);
    const credentials = await browserCredentials(actor);
    if (!credentials) {
      inconclusive(`no session found in ${name}'s browser, so the action could not be issued as them`);
    }
    prepared.push({ name, credentials });
  }
  if (prepared.length < 2) inconclusive('fewer than two actors, so nothing was contended');
  const request = named.request(action, input);
  const requestUrl = request?.url;
  if (!requestUrl) inconclusive('could not resolve where to send the action for this backend');
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

async function expectCallOutcomes({ input, capabilities }: ActorArguments) {
  const result = capabilities['named-actions'].lastCalls.get();
  if (!result) fail('no callConcurrently ran before this assertion');
  const accepted = result.outcomes.filter(outcome => outcome.ok).length;
  if (input.accepted !== undefined && accepted !== input.accepted) {
    const detail = result.outcomes.map(outcome => `${outcome.name}:${outcome.status}`).join(' ');
    fail(`${accepted} of ${result.fired} concurrent "${result.action}" requests were accepted, expected exactly `
      + `${input.accepted} (${detail}) — issued within ${result.ms}ms`);
  }
  return { accepted, fired: result.fired, outcomes: result.outcomes };
}

async function runScript({ input, capabilities, signal }: ActorArguments) {
  const files = capabilities['application-files'];
  const subprocess = capabilities.subprocess;
  if (!files.root) inconclusive('grader was not told the app directory (--app)');
  const root = resolve(files.root);
  const path = resolve(root, input.script);
  const fromRoot = relative(root, path);
  if (!fromRoot || fromRoot === '..' || fromRoot.startsWith(`..${process.platform === 'win32' ? '\\' : '/'}`)
    || isAbsolute(fromRoot)) {
    fail(`${input.script} is not a script inside the application directory`);
  }
  if (!existsSync(path)) fail(`the app does not ship ${input.script} — the spec requires it`);
  try {
    execFileSync('node', [path, ...(input.args ?? []).map(files.expand)], {
      cwd: files.root,
      encoding: 'utf8',
      stdio: 'pipe',
      timeout: input.timeoutMs ?? 60000,
    });
  } catch (error) {
    const infrastructure = harnessProcessFailure(error);
    // The harness successfully launched Node and the application-owned script
    // then exceeded its contract deadline. That is an application defect, not
    // missing measurement evidence. Spawn/permission failures remain harness
    // failures because the script never ran.
    if (infrastructure && errorField(error, 'code') !== 'ETIMEDOUT') throw error;
    const output = String(errorField(error, 'stdout') ?? '')
      + String(errorField(error, 'stderr') ?? '');
    fail(`${input.script} failed: ${output.trim().slice(-200)
      || String(errorField(error, 'message') ?? error)}`);
  }
  await subprocess.sleep(input.settleMs ?? 3000, signal);
  return { script: input.script, completed: true };
}

type NamedFetch = NamedActionsCapability['fetch'];

export function createNamedActionsCapability({
  actions,
  backend,
  url,
  spacetime,
  lastCalls,
  sleep,
  fetchImpl = fetch as unknown as NamedFetch,
  now = () => Date.now(),
}: {
  readonly actions?: readonly NamedAction[];
  readonly backend: string;
  readonly url?: string | null;
  readonly spacetime?: unknown;
  readonly lastCalls: {
    get(): ConcurrentCallResult | null;
    set(result: ConcurrentCallResult): void;
  };
  readonly sleep: NamedActionsCapability['sleep'];
  readonly fetchImpl?: NamedFetch;
  readonly now?: () => number;
}): NamedActionsCapability {
  const adapter = STACK_ADAPTER_REGISTRY.get(backend);
  if (!adapter) throw new Error(`unknown stack adapter "${backend}"`);
  return Object.freeze({
    resolve: (id: string) => (actions ?? []).find(action => action.id === id) ?? null,
    request(action: NamedAction, input: unknown) {
      return executeStackCapability(adapter, 'named-action', 'request',
        { action, input, spacetime, url }) as NamedActionRequest | null;
    },
    fetch: fetchImpl,
    lastCalls: Object.freeze({ get: lastCalls.get, set: lastCalls.set }),
    now,
    sleep,
  });
}

function contractActorAction<Result>(
  implementation: (arguments_: ActorArguments) => Result | Promise<Result>,
): ActionImplementation {
  return (arguments_: ActionImplementationArguments) => implementation({
    input: arguments_.input as CompiledActorTransportInput,
    capabilities: arguments_.capabilities as unknown as ActorTransportCapabilities,
    signal: arguments_.signal,
  });
}

export const ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS = Object.freeze({
  callAction: contractActorAction(browserApplicationBoundary(callAction)),
  callConcurrently: contractActorAction(callConcurrently),
  createRoom: contractActorAction(browserApplicationBoundary(createRoom)),
  ensureRegistered: contractActorAction(browserApplicationBoundary(ensureRegistered)),
  ensureSignedIn: contractActorAction(browserApplicationBoundary(ensureSignedIn)),
  enterRoom: contractActorAction(browserApplicationBoundary(enterRoom)),
  expectCallOutcomes: contractActorAction(expectCallOutcomes),
  expectActionOutcome: contractActorAction(expectActionOutcome),
  expectForgeryRejected: contractActorAction(expectForgeryRejected),
  expectNotReceived: contractActorAction(expectNotReceived),
  expectReceived: contractActorAction(browserApplicationBoundary(expectReceived)),
  expectReplayRejected: contractActorAction(expectReplayRejected),
  forgeWrite: contractActorAction(browserApplicationBoundary(forgeWrite)),
  register: contractActorAction(browserApplicationBoundary(register)),
  replayAs: contractActorAction(browserApplicationBoundary(replayAs)),
  runScript: contractActorAction(runScript),
  scheduleMessage: contractActorAction(browserApplicationBoundary(scheduleMessage)),
  send: contractActorAction(browserApplicationBoundary(send)),
  sendMany: contractActorAction(browserApplicationBoundary(sendMany)),
  signIn: contractActorAction(browserApplicationBoundary(signIn)),
  signUp: contractActorAction(browserApplicationBoundary(signUp)),
});

if (Object.keys(ACTOR_TRANSPORT_ACTION_IMPLEMENTATIONS).sort().join('\0')
  !== ACTOR_TRANSPORT_ACTION_IDS.join('\0')) {
  throw new Error('actor/transport action implementation registry does not match its declared action ids');
}
