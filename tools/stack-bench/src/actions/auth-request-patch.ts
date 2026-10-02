import { createHash } from 'node:crypto';
import type { Page, Request, Route } from 'playwright';
import { inconclusive } from './actor-action-runtime.js';
import { ActionApplicationFailure } from './action-contract.js';
import { browserApplicationBoundary } from './browser-boundary.js';
import { startSpacetimeAuthPatch, startSpacetimeAuthWriteCapture } from '../stacks/backends/spacetime-browser-session.js';
import { withBrowserRequest } from './browser-request.js';
import { installSocketIoAuthCapture, socketIoAuthCapture, socketIoTransport } from './socketio-auth-capture.js';

export interface AuthRequestPatch {
  readonly fields?: Readonly<Record<string, unknown>>;
  readonly password?: unknown;
}

// These identities describe a submitted write without retaining credentials in
// action evidence. Selection means "this write in the flow", never "the signup".
export interface AuthWrite {
  readonly transport: 'http' | 'spacetime-websocket' | 'convex-websocket' | 'socketio';
  readonly destination: string;
  readonly shape: string;
}
export interface AuthWriteTarget {
  readonly writes: readonly AuthWrite[];
  readonly index: number;
  readonly readEndpoints?: readonly string[];
}
const writeTargets = new WeakMap<object, AuthWriteTarget>();
const writeStops = new WeakMap<object, () => void>();
export const hasAuthWriteTarget = (page: object): boolean => writeTargets.has(page);
export const stopAuthWriteInventory = (page: object): void => writeStops.get(page)?.();

type AuthSubmit = () => Promise<Record<string, unknown>>;
const submitCaptures = new WeakMap<object, { used: boolean; capture: (submit: AuthSubmit) => ReturnType<AuthSubmit> }>();

// Registered signup prepares the form before it consumes this one-shot hook.
// Direct capture helpers retain their immediate capture boundary.
export async function withAuthSubmitCapture<T>(page: object, run: () => Promise<T>,
  capture: (submit: AuthSubmit) => ReturnType<AuthSubmit>): Promise<T> {
  if (submitCaptures.has(page)) throw new Error('Authentication submit capture is already active');
  const hook = { used: false, capture };
  submitCaptures.set(page, hook);
  try {
    const result = await run();
    if (!hook.used) throw new Error('Authentication submit capture was not reached');
    return result;
  } finally { submitCaptures.delete(page); }
}

export async function captureAuthSubmit(page: object, submit: AuthSubmit): ReturnType<AuthSubmit> {
  const hook = submitCaptures.get(page);
  if (!hook) return submit();
  if (hook.used) throw new Error('Authentication submit capture was already consumed');
  hook.used = true;
  return hook.capture(async () => {
    // The account can appear before its authentication reconnect. Give the
    // baseline the same minimum observation window as the patched signup.
    const settled = new Promise<void>(resolve => setTimeout(resolve, 2000));
    try { return await submit(); } finally { await settled; }
  });
}

export async function withAuthWriteTarget<T>(page: object, target: AuthWriteTarget, submit: () => Promise<T>): Promise<T> {
  if (writeTargets.has(page)) throw new Error('Authentication write target is already active');
  writeTargets.set(page, target);
  try { return await submit(); } finally { writeTargets.delete(page); }
}

const hash = (value: string) => createHash('sha256').update(value).digest('hex');
// A write's destination without per-account values: identifier-like path
// segments and query values differ between the baseline account and a probe.
function writeDestination(method: string, raw: string): string {
  const url = new URL(raw);
  const identifier = (segment: string) => /^\d+$/.test(segment)
    || /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(segment)
    || /^[0-9a-f]{16,}$/i.test(segment) || segment.length >= 20 && /\d/.test(segment) && /[a-z]/i.test(segment);
  const path = url.pathname.split('/').map(segment => identifier(decodeURIComponent(segment)) ? ':id' : segment).join('/');
  return JSON.stringify([method, url.origin, path, [...new Set(url.searchParams.keys())].sort()]);
}
function valueShape(value: unknown): unknown {
  if (value === null) return 'null';
  if (Array.isArray(value)) return value.map(valueShape);
  if (typeof value === 'object') return Object.fromEntries(Object.entries(value).sort(([a], [b]) => a.localeCompare(b))
    .map(([key, child]) => [key, valueShape(child)]));
  return typeof value;
}

// The fields-only path already knows its exact write. Credential equality cannot
// identify a bodyless finalizer or a form that derives its password in the browser.
function patchWriteFields(body: unknown, patch: AuthRequestPatch, parameters?: readonly CallParameter[]) {
  const fields = requestedChange(patch);
  if (Object.hasOwn(patch, 'password')) throw new Error('A write target only supports authority fields');
  if (!Array.isArray(body)) {
    if (body !== null && (typeof body !== 'object' || !body)) throw new Error('Unsupported write body');
    if (body && Object.values(body).some(value => value !== null && typeof value === 'object')) {
      throw new Error('Authority field location is unproved');
    }
    return { value: { ...body as Record<string, unknown> | null, ...fields }, shape: body === null ? 'bodyless' : 'object' };
  }
  const value = structuredClone(body);
  return { value, ...putFields(value, fields, parameters) };
}

// A platform's own password endpoints, supplied by its stack adapter. The
// application may encode the typed credentials in any way there, so the endpoint,
// not a credential value, identifies the request. The patch reaching it is valid.
// Returns the changed body, null when the endpoint's body holds no credential, or
// undefined when the request is not one of those endpoints.
export type PlatformAuthPatch = (url: string, body: unknown, patch: AuthRequestPatch)
  => { body: string; shape: string } | null | undefined;

type PatchReceipt = { shape: string; status?: number; success?: boolean; bodySha256: string;
  transport?: 'convex-websocket' | 'spacetime-websocket' | 'socketio';
  absentParameters?: string[] };
type SocketPatch = {
  captureAll?: boolean;
  nativeCapture?: boolean;
  change(body: unknown): ReturnType<typeof patchAuthRequest>;
  receipt(value: PatchReceipt): void;
  fail(): void;
};
const socketPatches = new WeakMap<object, { active?: SocketPatch }>();

// Install before navigation: Playwright cannot route an already-open socket.
// Context routing lets the later response-loss gate replace this passive route.
export async function installAuthWebSocketCapture(page: Page): Promise<void> {
  const state: { active?: SocketPatch } = {};
  socketPatches.set(page, state);
  await installSocketIoAuthCapture(page);
  page.on('websocket', socket => socket.on('framesent', () => {
    const owner = state.active;
    if (owner?.captureAll && !/\/api\/[^/]+\/sync(?:\?|$)/.test(socket.url())
      && !socketIoTransport(socket.url(), 'websocket')
      && !(owner.nativeCapture && /\/v1\/database\/[^/]+\/subscribe(?:\?|$)/.test(socket.url()))) owner.fail();
  }));
  await page.context().routeWebSocket(/\/api\/[^/]+\/sync(?:\?|$)/, client => {
    const server = client.connectToServer();
    let awaiting: { owner: SocketPatch; id: number; type: string; changed: NonNullable<ReturnType<typeof patchAuthRequest>> } | undefined;
    client.onMessage(data => {
      let message: Record<string, unknown> | undefined;
      try { message = JSON.parse(String(data)); } catch { /* unrelated frame */ }
      const owner = state.active;
      if (owner && message && ['Mutation', 'Action'].includes(String(message.type))) {
        try {
          const changed = owner.change(message);
          if (changed || owner.captureAll) {
            if (!Number.isSafeInteger(message.requestId) || awaiting) { owner.fail(); return; }
            awaiting = { owner, id: message.requestId as number, type: `${message.type}Response`,
              changed: changed ?? { body: String(data), shape: 'captured' } };
            server.send(changed?.body ?? data);
            return;
          }
        } catch { owner.fail(); return; }
      }
      server.send(data);
    });
    server.onMessage(data => {
      if (awaiting) {
        let message: Record<string, unknown> | undefined;
        try { message = JSON.parse(String(data)); } catch { /* unrelated frame */ }
        if (message?.type === awaiting.type && message.requestId === awaiting.id) {
          const { owner, changed } = awaiting;
          if (typeof message.success !== 'boolean') owner.fail();
          else owner.receipt({ shape: changed.shape, success: message.success,
            transport: 'convex-websocket', bodySha256: createHash('sha256').update(changed.body).digest('hex') });
          awaiting = undefined;
        }
      }
      client.send(data);
    });
    client.onClose(async (code, reason) => {
      awaiting?.owner.fail(); await server.close({ code, reason }).catch(() => awaiting?.owner.fail());
    });
    server.onClose(async (code, reason) => {
      awaiting?.owner.fail(); await client.close({ code, reason }).catch(() => awaiting?.owner.fail());
    });
  });
}

// Module case conversion may render one identifier as signUp, sign_up, or signup.
const sameIdentifier = (left: unknown, right: string): boolean => typeof left === 'string'
  && left.toLowerCase().replaceAll('_', '') === right.toLowerCase().replaceAll('_', '');

// A positional parameter: the field names its declared object type establishes, whether
// that object is optional (sent as `{ some: object }`), and whether its type could hold
// fields the schema does not show directly (open).
export type CallParameter = { readonly name: string; readonly fields?: readonly string[]; readonly optional?: boolean;
  readonly open?: boolean };

// Locate values submitted by the real form, not a guessed route or credential key.
// Positional arguments name nothing, so a field goes where the interface declares
// it: a parameter of that name, or the one object parameter whose type declares it.
// A field the interface provably lacks is reported, not sent; a type that could
// hide it leaves the location unknown.
export function patchAuthRequest(body: unknown, username: string, password: string, patch: AuthRequestPatch,
  parameters?: readonly CallParameter[]) {
  const changed = patchAuthValues(body, username, password, patch, parameters);
  if (!changed) return null;
  const { value, ...receipt } = changed;
  return { body: JSON.stringify(value), ...receipt };
}

function patchAuthValues(body: unknown, username: string, password: string | undefined, patch: AuthRequestPatch,
  parameters?: readonly CallParameter[]) {
  if (password === undefined && Object.hasOwn(patch, 'password')) throw new Error('Credential value is unproved');
  const fields = requestedChange(patch);
  const copy = structuredClone(body);
  const matches: { container: Record<string, unknown> | unknown[]; passwordKey?: string }[] = [];
  const visit = (value: unknown): void => {
    if (!value || typeof value !== 'object') return;
    const entries = Object.entries(value);
    const users = entries.filter(([, v]) => v === username);
    const secrets = password === undefined ? [] : entries.filter(([, v]) => v === password);
    if (users.length && (password === undefined || secrets.length)) {
      if (users.length !== 1 || password !== undefined &&
        (secrets.length !== 1 || users[0]![0] === secrets[0]![0])) {
        throw new Error('Ambiguous credential values');
      }
      matches.push({ container: value as Record<string, unknown>, passwordKey: secrets[0]?.[0] });
    }
    for (const [, child] of entries) visit(child);
  };
  visit(copy);
  if (!matches.length) return null;
  if (matches.length !== 1) throw new Error('Multiple credential containers');
  const { container, passwordKey } = matches[0]!;
  if (Object.hasOwn(patch, 'password')) {
    Object.defineProperty(container, passwordKey!,
      { value: patch.password, enumerable: true, writable: true, configurable: true });
  }
  return { value: copy, ...putFields(container, fields, parameters) };
}

function putFields(container: Record<string, unknown> | unknown[], fields: Readonly<Record<string, unknown>>,
  parameters?: readonly CallParameter[]) {
  const absentParameters: string[] = [];
  if (Array.isArray(container)) {
    if (Object.keys(fields).length && parameters?.length !== container.length) {
      throw new Error('Positional credential fields need the interface parameters');
    }
    for (const [key, value] of Object.entries(fields)) {
      const index = parameters!.findIndex(parameter => sameIdentifier(parameter.name, key));
      const owners = parameters!.flatMap((parameter, at) => {
        const field = parameter.fields?.find(name => sameIdentifier(name, key));
        return field ? [{ at, field }] : [];
      });
      if (index >= 0) container[index] = value;
      else if (owners.length === 1) {
        const { at, field } = owners[0]!, optional = parameters![at]!.optional;
        // An absent optional object is not built here: its other required values would be invented.
        const argument = optional ? (container[at] as { some?: unknown } | null)?.some : container[at];
        if (!argument || typeof argument !== 'object' || Array.isArray(argument)) {
          throw new Error('Declared credential field has no object argument');
        }
        const changed = Object.defineProperty({ ...argument }, field,
          { value, enumerable: true, writable: true, configurable: true });
        container[at] = optional ? { some: changed } : changed;
      } else if (owners.length) throw new Error('Ambiguous credential field location');
      else if (parameters!.some(parameter => parameter.open)) throw new Error('Credential field location unknown');
      else absentParameters.push(key);
    }
  } else for (const [key, value] of Object.entries(fields)) {
    Object.defineProperty(container, key, { value, enumerable: true, writable: true, configurable: true });
  }
  return { shape: Array.isArray(container) ? 'positional' : 'object',
    ...(absentParameters.length ? { absentParameters } : {}) };
}

function requestedChange(patch: AuthRequestPatch): Readonly<Record<string, unknown>> {
  const fields = patch.fields ?? {};
  if (Object.keys(patch).some(key => key !== 'fields' && key !== 'password') || Array.isArray(fields)
    || typeof fields !== 'object' || !Object.keys(fields).length && !Object.hasOwn(patch, 'password')) {
    throw new Error('Expected an authentication request change');
  }
  return fields;
}

const SCALARS = new Set(['Bool', 'I8', 'U8', 'I16', 'U16', 'I32', 'U32', 'I64', 'U64', 'I128', 'U128',
  'I256', 'U256', 'F32', 'F64', 'String']);

// A SpacetimeDB function call sends positional arguments; its module schema
// names them and their object types. Read it through the page's own network.
async function callParameters(request: Request): Promise<CallParameter[] | undefined> {
  const call = new URL(request.url()).pathname.match(/^\/v1\/database\/([^/]+)\/call\/([^/]+)$/);
  if (!call) return undefined;
  try {
    const schema = await withBrowserRequest(request.frame().page().request, async api => {
      const response = await api.get(new URL(`/v1/database/${call[1]}/schema?version=9`, request.url()).href,
        { headers: request.headers().authorization ? { authorization: request.headers().authorization! } : {}, timeout: 10_000 });
      return response.ok() ? response.json() : null;
    }) as { typespace?: { types?: unknown[] } } | null;
    if (!schema) return undefined;
    const name = decodeURIComponent(call[2]!);
    const found: unknown[][] = [];
    const visit = (value: unknown): void => {
      if (!value || typeof value !== 'object') return;
      const entry = value as { name?: unknown; params?: { elements?: unknown } };
      if (sameIdentifier(entry.name, name) && Array.isArray(entry.params?.elements)) found.push(entry.params.elements);
      for (const child of Object.values(value)) visit(child);
    };
    visit(schema);
    type Element = { name?: { some?: unknown }; algebraic_type?: unknown } | null;
    const names = (elements: unknown[]) => {
      const list = elements.map(element => (element as Element)?.name?.some);
      return list.every(item => typeof item === 'string') ? list as string[] : undefined;
    };
    // A type is inline or a reference into the module's typespace; an unresolved one stays unknown.
    const resolve = (type: unknown) => {
      const ref = (type as { Ref?: unknown } | null)?.Ref;
      return (typeof ref === 'number' ? schema.typespace?.types?.[ref] : type) as Record<string, unknown> | undefined;
    };
    const members = (type: Record<string, unknown> | undefined, kind: 'Product' | 'Sum') => {
      const list = (type?.[kind] as { elements?: unknown; variants?: unknown } | undefined)?.[kind === 'Product' ? 'elements' : 'variants'];
      return Array.isArray(list) ? list as Element[] : undefined;
    };
    // Plain values cannot hold a named field: scalars, units, and choices among plain values (an optional string).
    const plain = (type: unknown, depth = 0): boolean => {
      const resolved = resolve(type);
      if (!resolved || depth > 8) return false;
      if (Object.keys(resolved).some(key => SCALARS.has(key))) return true;
      if (members(resolved, 'Product')?.length === 0) return true;
      return members(resolved, 'Sum')?.every(variant => plain(variant?.algebraic_type, depth + 1)) ?? false;
    };
    const object = (type: unknown) => {
      const elements = members(resolve(type), 'Product'), fields = elements && names(elements);
      return fields && { fields, open: !elements.every(item => plain(item?.algebraic_type)) };
    };
    // An option is a choice of `some` object or unit `none`.
    const option = (type: unknown) => {
      const variants = members(resolve(type), 'Sum');
      const some = variants?.find(variant => variant?.name?.some === 'some');
      return variants?.length === 2 && some && variants.some(variant => variant?.name?.some === 'none'
        && members(resolve(variant.algebraic_type), 'Product')?.length === 0) ? object(some.algebraic_type) : undefined;
    };
    const describe = (name: string, { algebraic_type: type }: NonNullable<Element>): CallParameter => {
      if (plain(type)) return { name };
      const direct = object(type), found = direct ?? option(type);
      return found ? { name, fields: found.fields, optional: !direct, open: found.open } : { name, open: true };
    };
    if (found.length !== 1 || !found[0]!.length) return undefined;
    return names(found[0]!)?.map((name, at) => describe(name, (found[0]![at] ?? {}) as NonNullable<Element>));
  } catch { return undefined; }
}

export async function withAuthWriteInventory<T>(page: Page, submit: () => Promise<T>, readEndpoints: readonly string[] = []) {
  const { result, writes } = await captureAuthWrites(page, submit, undefined, readEndpoints);
  return { result, writes };
}

// Observe writes dispatched during the UI action and while their receipts drain.
// A no-op control is valid; this does not cover a later, independently queued task.
export async function withWriteCompletion<T>(page: Page, submit: () => Promise<T>, readEndpoints: readonly string[] = []) {
  const { result, writes } = await captureAuthWrites(page, submit, undefined, readEndpoints, true);
  return { result, writes };
}

interface WriteProbe { target: AuthWriteTarget; patch: AuthRequestPatch; username: string; password: string;
  platformPatch?: PlatformAuthPatch | null; complete?: (receipt: PatchReceipt) => Promise<void> }

async function withAuthWriteProbe<T>(page: Page, submit: () => Promise<T>, probe: WriteProbe) {
  const { result, requestPatch } = await captureAuthWrites(page, submit, probe, probe.target.readEndpoints);
  if (!requestPatch) inconclusive('replay-unavailable', { actor: 'authentication form', detail: 'The selected signup write has no terminal receipt' });
  return { ...result, requestPatch };
}

async function captureAuthWrites<T>(page: Page, submit: () => Promise<T>,
  probe?: WriteProbe, readEndpoints: readonly string[] = [], completionOnly = false) {
  const writes: AuthWrite[] = [], pending: Promise<void>[] = [];
  let failed = false, stopped = false, requestPatch: PatchReceipt | undefined, selected = 0;
  let finishTarget!: () => void;
  const targetReady = new Promise<void>(resolve => { finishTarget = resolve; });
  const fail = () => { failed = true; finishTarget(); };
  const recordPatch = (receipt: PatchReceipt) => { requestPatch = receipt; finishTarget(); };
  if (page.context().serviceWorkers().length) fail();
  if (probe && (!Number.isSafeInteger(probe.target.index) || probe.target.index < 0
    || probe.target.index >= probe.target.writes.length || Object.hasOwn(probe.patch, 'password'))) fail();
  const visit = (write: AuthWrite): boolean => {
    if (stopped) return false;
    const index = writes.length;
    writes.push(write);
    if (writes.length > 200) fail();
    if (!probe) return false;
    if (failed) return false;
    const expected = probe.target.writes[index];
    if (!expected || expected.transport !== write.transport || expected.destination !== write.destination || expected.shape !== write.shape) {
      // Writes after the patched target may follow from the claim itself (an app
      // that grants the claimed role can do more). They are recorded, not refused.
      // A repeat of the target itself would be unpatched and could undo the claim.
      const target = probe.target.writes[probe.target.index]!;
      if (selected === 1 && index > probe.target.index
        && !(write.transport === target.transport && write.destination === target.destination)) return false;
      fail(); return false;
    }
    return index === probe.target.index && ++selected === 1;
  };
  const native = await startSpacetimeAuthWriteCapture(page, (route, args) => {
    const url = new URL(route.url);
    // Socket authentication tokens vary between fresh accounts. The leased
    // database, origin, operation, flags and argument schema identify the write.
    return visit({ transport: 'spacetime-websocket',
      destination: hash(JSON.stringify([url.origin, url.pathname, route.operation, route.flags])),
      shape: hash(JSON.stringify([route.parameters, valueShape(args)])) })
      ? patchWriteFields(args, probe!.patch, route.parameters.map(name => ({ name }))) : null;
  }, (receipt, changed) => { if (changed) recordPatch(receipt); }, fail);
  const sockets = socketPatches.get(page);
  if (sockets?.active) { native?.dispose(); throw new Error('Authentication request capture is already active'); }
  let socketFinish: (() => void) | undefined, socketTimer: ReturnType<typeof setTimeout> | undefined;
  let socketChanged = false;
  if (sockets) sockets.active = {
    captureAll: true, nativeCapture: Boolean(native),
    change(body) {
      if (stopped) return null;
      const message = body as { type?: unknown; udfPath?: unknown; args?: unknown };
      const args = message.args;
      const target = visit({ transport: 'convex-websocket',
        destination: hash(JSON.stringify([message.type, message.udfPath])), shape: hash(JSON.stringify(valueShape(args))) });
      pending.push(new Promise<void>(resolve => {
        socketFinish = resolve; socketTimer = setTimeout(() => { fail(); resolve(); }, 10_000);
      }));
      socketChanged = target;
      if (!target) return null;
      const located = patchAuthRequest(body, probe!.username, probe!.password, probe!.patch);
      if (located) return located;
      if (!Array.isArray(args) || args.length !== 1 || !args[0] || Array.isArray(args[0]) || typeof args[0] !== 'object') {
        fail(); throw new Error('Unsupported native authority field location');
      }
      const changed = patchWriteFields(args[0], probe!.patch);
      return { body: JSON.stringify({ ...message, args: [changed.value] }), shape: 'convex-args-object' };
    },
    receipt(value) { if (socketChanged) recordPatch(value); clearTimeout(socketTimer); socketFinish?.(); },
    fail() { fail(); clearTimeout(socketTimer); socketFinish?.(); },
  };
  if (writeStops.has(page)) { native?.dispose(); throw new Error('Authentication write inventory is already active'); }
  const socketIo = socketIoAuthCapture(page)?.activate({
    connect(endpoint, body, packet) {
      const target = visit({ transport: 'socketio', destination: hash(endpoint), shape: hash(JSON.stringify(valueShape(body))) });
      const changed = target ? patchWriteFields(body, probe!.patch) : null;
      const sent = changed ? `40${JSON.stringify(changed.value)}` : packet;
      let finish!: () => void;
      pending.push(new Promise<void>(resolve => { finish = resolve; }));
      return { packet: sent, finish(success) {
        if (success === null) fail();
        else if (target) recordPatch({ shape: changed!.shape, success, transport: 'socketio', bodySha256: hash(sent) });
        finish();
      } };
    },
    fail,
  });
  writeStops.set(page, () => {
    stopped = true; native?.stop(); socketIo?.stop();
    if (sockets?.active) sockets.active.captureAll = false;
  });
  const handler = async (route: Route) => {
    if (stopped) return route.fallback();
    const request = route.request();
    if (socketIo && socketIoTransport(request.url(), 'polling')) return route.fallback();
    if (['GET', 'HEAD', 'OPTIONS'].includes(request.method())) return route.fallback();
    // Only the adapter's exact leased endpoint carries a platform read guarantee.
    // An app proxy with the same path can still perform its own writes.
    if (request.method() === 'POST' && readEndpoints.includes(request.url())) return route.fallback();
    const raw = request.postData();
    let body: unknown = null;
    const contentType = request.headers()['content-type']?.split(';')[0]?.trim();
    try {
      if (raw) {
        if (completionOnly) body = raw;
        else {
          if (!contentType || !/^application\/(?:[\w.-]+\+)?json$/i.test(contentType)) throw new Error('Opaque write body');
          body = request.postDataJSON();
        }
      }
    } catch { fail(); return route.fallback(); }
    const target = visit({ transport: 'http', destination: hash(writeDestination(request.method(), request.url())),
      shape: hash(JSON.stringify(valueShape(body))) });
    if (!target) {
      pending.push(request.response().then(async response => {
        if (!response) { fail(); return; }
        const headers = response.headers();
        // Chromium can leave finished() pending for a zero-length response.
        // Its complete headers prove the empty body without waiting for bytes.
        const empty = headers['content-length'] === '0' && !headers['transfer-encoding'];
        // A redirect after a form post (post/redirect/get) is a completed write.
        const redirect = response.status() >= 300 && response.status() < 400;
        if (!redirect && !empty && await response.finished() !== null
          || completionOnly && response.status() === 202) fail();
      }).catch(fail));
      return route.fallback();
    }
    try {
      const parameters = Array.isArray(body) ? await callParameters(request) : undefined;
      requestedChange(probe!.patch);
      const platform = probe!.platformPatch?.(request.url(), body, probe!.patch);
      const located = platform === undefined
        ? patchAuthRequest(body, probe!.username, probe!.password, probe!.patch, parameters) : platform;
      if (platform === null) throw new Error('Platform write holds no credential');
      const changed = located ? { ...located, value: JSON.parse(located.body) as unknown }
        : patchWriteFields(body, probe!.patch, parameters);
      const sentBody = JSON.stringify(changed.value);
      const sent = withBrowserRequest(page.request, async api => {
        const response = await api.fetch(request, { data: sentBody,
          headers: { ...request.headers(), ...(!raw ? { 'content-type': 'application/json' } : {}) },
          maxRedirects: 0, maxRetries: 0, timeout: 10_000 });
        if (response.status() >= 300 && response.status() < 400) fail();
        recordPatch({ shape: changed.shape, status: response.status(), bodySha256: hash(sentBody),
          ...('absentParameters' in changed && changed.absentParameters ? { absentParameters: changed.absentParameters } : {}) });
        await route.fulfill({ response });
      }, false).catch(async () => { fail(); await route.abort().catch(() => {}); });
      pending.push(sent); await sent;
    } catch { fail(); await route.abort().catch(() => {}); }
  };
  try {
    await page.route('**/*', handler);
    let result: T | undefined, submissionFailure: unknown;
    try { result = await browserApplicationBoundary(submit)(undefined); } catch (error) { submissionFailure = error; }
    if (probe && !requestPatch && !failed) {
      let targetTimer: ReturnType<typeof setTimeout> | undefined;
      try {
        await Promise.race([targetReady, new Promise<void>(resolve => {
          targetTimer = setTimeout(() => { fail(); resolve(); }, 10_000);
        })]);
      } finally { clearTimeout(targetTimer); }
    }
    const drain = async () => {
      let drainTimer: ReturnType<typeof setTimeout> | undefined;
      try {
        const receipts = async () => {
          let consumed = 0;
          do {
            const batch = pending.slice(consumed);
            consumed = pending.length;
            await Promise.all([...batch, native?.finish()]);
          } while (!failed && consumed < pending.length);
        };
        await Promise.race([receipts(), new Promise<void>(resolve => {
          drainTimer = setTimeout(() => { fail(); resolve(); }, 10_000);
        })]);
      } finally { clearTimeout(drainTimer); }
    };
    await drain();
    if (!failed && !submissionFailure && requestPatch && probe?.complete) {
      try { await probe.complete(requestPatch); } catch (error) { submissionFailure = error; }
      await drain();
    }
    if (!probe && !failed && submissionFailure) throw submissionFailure;
    if (failed || !completionOnly && !writes.length || probe && (selected !== 1 || !requestPatch)) {
      if (completionOnly) inconclusive('transport-incomplete', {});
      // Writes are hashed (transport, destination, argument shape), so the inventory holds no credentials.
      // A fresh client that cannot repeat the baseline sequence may have met a transport resend under
      // load, so its suite may run once more from fresh state.
      inconclusive('replay-unavailable', { actor: 'authentication form', detail: 'Could not prove the signup write sequence and one complete target request' },
        { observation: { writes, ...(probe ? { baseline: probe.target.writes, target: probe.target.index } : {}) },
          retryable: Boolean(probe) });
    }
    if (submissionFailure) throw submissionFailure;
    return { result: result as T, writes, requestPatch };
  } finally {
    clearTimeout(socketTimer);
    writeStops.delete(page);
    if (sockets) sockets.active = undefined;
    native?.dispose();
    socketIo?.dispose();
    await page.unroute('**/*', handler);
  }
}

export async function withAuthRequestPatch<T>(page: Pick<Page, 'route' | 'unroute'>,
  username: string, password: string, patch: AuthRequestPatch, submit: () => Promise<T>,
  platformPatch?: PlatformAuthPatch | null, nativeKind?: 'signup' | 'signin', target = writeTargets.get(page),
  complete?: (receipt: PatchReceipt) => Promise<void>) {
  if (target) return withAuthWriteProbe(page as Page, submit, { target, patch, username, password, platformPatch, complete });
  let matches = 0, error = false;
  const pending: Promise<void>[] = [];
  let receipt: PatchReceipt | undefined;
  let finishSocket: (() => void) | undefined, socketTimeout: ReturnType<typeof setTimeout> | undefined;
  const sockets = socketPatches.get(page);
  if (sockets?.active) throw new Error('Authentication request patch is already active');
  if (sockets) sockets.active = {
    change(body) {
      const changed = patchAuthRequest(body, username, password, patch);
      if (changed && ++matches !== 1) { error = true; throw new Error('Multiple credential requests'); }
      if (changed) pending.push(new Promise<void>(resolve => {
        finishSocket = resolve;
        socketTimeout = setTimeout(() => { error = true; resolve(); }, 30_000);
      }));
      return changed;
    },
    receipt(value) { receipt = value; clearTimeout(socketTimeout); finishSocket?.(); },
    fail() { error = true; clearTimeout(socketTimeout); finishSocket?.(); },
  };
  let spacetime: Awaited<ReturnType<typeof startSpacetimeAuthPatch>>;
  try {
    spacetime = await startSpacetimeAuthPatch(page, username, password, (args, parameters, observedPassword) => {
      const changed = patchAuthValues(args, username, observedPassword === null ? undefined : observedPassword ?? password, patch, parameters);
      if (changed && ++matches !== 1) throw new Error('Multiple credential requests');
      return changed;
    }, () => { error = true; }, nativeKind, !Object.hasOwn(patch, 'password'));
  } catch (error) {
    if (sockets) sockets.active = undefined;
    throw error;
  }
  const handler = async (route: Route) => {
    const request = route.request();
    let body: unknown;
    try { body = request.postDataJSON(); } catch { return route.fallback(); }
    let changed: ReturnType<typeof patchAuthRequest>;
    const parameters = Array.isArray(body) && Object.keys(patch.fields ?? {}).length
      ? await callParameters(request) : undefined;
    try {
      requestedChange(patch);
      const platform = platformPatch?.(request.url(), body, patch);
      changed = platform === undefined ? patchAuthRequest(body, username, password, patch, parameters) : platform;
    } catch { error = true; return route.abort(); }
    if (!changed) return route.fallback();
    const contentType = request.headers()['content-type']?.split(';')[0]?.trim();
    if (request.method() !== 'POST' || !contentType || !/^application\/(?:[\w.-]+\+)?json$/i.test(contentType)
      || ++matches !== 1) { error = true; return route.abort(); }
    const sent = (async () => {
      try {
        // Preserve the actual route, headers and native envelope. No retries or redirects.
        await withBrowserRequest(request.frame().page().request, async api => {
          const response = await api.fetch(request, { data: changed.body, maxRedirects: 0, maxRetries: 0, timeout: 30_000 });
          const { body: sentBody, ...described } = changed;
          receipt = { ...described, status: response.status(),
            bodySha256: createHash('sha256').update(sentBody).digest('hex') };
          await route.fulfill({ response });
        }, false);
      } catch { error = true; await route.abort().catch(() => {}); }
    })();
    pending.push(sent);
    await sent;
  };
  try {
    await page.route('**/*', handler);
    let result: T | undefined, submissionFailure: { error: unknown } | undefined;
    try { result = await browserApplicationBoundary(submit)(undefined); }
    catch (error) { submissionFailure = { error }; }
    await Promise.all(pending);
    const spacetimeReceipt = await spacetime?.receipt();
    if (spacetimeReceipt) receipt = spacetimeReceipt;
    // A request the probe aborted itself explains whatever failure followed it.
    if (!error && submissionFailure && !(submissionFailure.error instanceof ActionApplicationFailure)) {
      throw submissionFailure.error;
    }
    if (error || matches !== 1 || !receipt || receipt.status !== undefined && receipt.status >= 300 && receipt.status < 400) {
      inconclusive('replay-unavailable', { actor: 'authentication form', detail: 'Could not prove one complete modified credential request' });
    }
    if (submissionFailure) throw submissionFailure.error;
    return { ...result, requestPatch: receipt };
  } finally {
    clearTimeout(socketTimeout);
    if (sockets) sockets.active = undefined;
    spacetime?.dispose();
    await page.unroute('**/*', handler);
  }
}
