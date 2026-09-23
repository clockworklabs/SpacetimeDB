import { createHash } from 'node:crypto';
import type { Page, Request, Route } from 'playwright';
import { inconclusive } from './actor-action-runtime.js';
import { ActionApplicationFailure } from './action-contract.js';
import { browserApplicationBoundary } from './browser-action-executors.js';
import { hasNetworkInterruption } from './network-interruption.js';

export interface AuthRequestPatch {
  readonly fields?: Readonly<Record<string, unknown>>;
  readonly password?: unknown;
}

type PatchReceipt = { shape: string; status?: number; success?: boolean; bodySha256: string; transport?: 'convex-websocket';
  absentParameters?: string[] };
type SocketPatch = {
  change(body: unknown): ReturnType<typeof patchAuthRequest>;
  receipt(value: PatchReceipt): void;
  fail(): void;
};
const socketPatches = new WeakMap<object, { active?: SocketPatch }>();

// Install before navigation: Playwright cannot route an already-open socket.
// Context routing lets the later response-loss gate replace this passive route.
export async function installAuthWebSocketCapture(page: Page): Promise<void> {
  // An interruptible context already routes every socket; a second route would bypass it.
  if (hasNetworkInterruption(page.context())) return;
  const state: { active?: SocketPatch } = {};
  socketPatches.set(page, state);
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
          if (changed) {
            if (!Number.isSafeInteger(message.requestId) || awaiting) { owner.fail(); return; }
            awaiting = { owner, id: message.requestId as number, type: `${message.type}Response`, changed };
            server.send(changed.body);
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
  const fields = patch.fields ?? {};
  if (Object.keys(patch).some(key => key !== 'fields' && key !== 'password') || Array.isArray(fields)
    || typeof fields !== 'object' || !Object.keys(fields).length && !Object.hasOwn(patch, 'password')) {
    throw new Error('Expected an authentication request change');
  }
  const copy = structuredClone(body);
  const matches: { container: Record<string, unknown> | unknown[]; passwordKey: string }[] = [];
  const visit = (value: unknown): void => {
    if (!value || typeof value !== 'object') return;
    const entries = Object.entries(value);
    const users = entries.filter(([, v]) => v === username), secrets = entries.filter(([, v]) => v === password);
    if (users.length && secrets.length) {
      if (users.length !== 1 || secrets.length !== 1 || users[0]![0] === secrets[0]![0]) {
        throw new Error('Ambiguous credential values');
      }
      matches.push({ container: value as Record<string, unknown>, passwordKey: secrets[0]![0] });
    }
    for (const [, child] of entries) visit(child);
  };
  visit(copy);
  if (!matches.length) return null;
  if (matches.length !== 1) throw new Error('Multiple credential containers');
  const { container, passwordKey } = matches[0]!;
  if (Object.hasOwn(patch, 'password')) {
    Object.defineProperty(container, passwordKey,
      { value: patch.password, enumerable: true, writable: true, configurable: true });
  }
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
  return { body: JSON.stringify(copy), shape: Array.isArray(container) ? 'positional' : 'object',
    ...(absentParameters.length ? { absentParameters } : {}) };
}

const SCALARS = new Set(['Bool', 'I8', 'U8', 'I16', 'U16', 'I32', 'U32', 'I64', 'U64', 'I128', 'U128',
  'I256', 'U256', 'F32', 'F64', 'String']);

// A SpacetimeDB function call sends positional arguments; its module schema
// names them and their object types. Read it through the page's own network.
async function callParameters(request: Request): Promise<CallParameter[] | undefined> {
  const call = new URL(request.url()).pathname.match(/^\/v1\/database\/([^/]+)\/call\/([^/]+)$/);
  if (!call) return undefined;
  try {
    const response = await request.frame().page().context().request.get(
      new URL(`/v1/database/${call[1]}/schema?version=9`, request.url()).href,
      { headers: request.headers().authorization ? { authorization: request.headers().authorization! } : {}, timeout: 10_000 });
    if (!response.ok()) return undefined;
    const name = decodeURIComponent(call[2]!);
    const found: unknown[][] = [];
    const visit = (value: unknown): void => {
      if (!value || typeof value !== 'object') return;
      const entry = value as { name?: unknown; params?: { elements?: unknown } };
      if (sameIdentifier(entry.name, name) && Array.isArray(entry.params?.elements)) found.push(entry.params.elements);
      for (const child of Object.values(value)) visit(child);
    };
    const schema = await response.json() as { typespace?: { types?: unknown[] } };
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

export async function withAuthRequestPatch<T>(page: Pick<Page, 'route' | 'unroute'>,
  username: string, password: string, patch: AuthRequestPatch, submit: () => Promise<T>) {
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
  const handler = async (route: Route) => {
    const request = route.request();
    let body: unknown;
    try { body = request.postDataJSON(); } catch { return route.fallback(); }
    let changed: ReturnType<typeof patchAuthRequest>;
    const parameters = Array.isArray(body) && Object.keys(patch.fields ?? {}).length
      ? await callParameters(request) : undefined;
    try { changed = patchAuthRequest(body, username, password, patch, parameters); }
    catch { error = true; return route.abort(); }
    if (!changed) return route.fallback();
    const contentType = request.headers()['content-type']?.split(';')[0]?.trim();
    if (request.method() !== 'POST' || !contentType || !/^application\/(?:[\w.-]+\+)?json$/i.test(contentType)
      || ++matches !== 1) { error = true; return route.abort(); }
    const sent = (async () => {
      try {
        // Preserve the actual route, headers and native envelope. No retries or redirects.
        const response = await route.fetch({ postData: changed.body, maxRedirects: 0, maxRetries: 0, timeout: 30_000 });
        const { body: sentBody, ...described } = changed;
        receipt = { ...described, status: response.status(),
          bodySha256: createHash('sha256').update(sentBody).digest('hex') };
        await route.fulfill({ response });
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
    await page.unroute('**/*', handler);
  }
}
