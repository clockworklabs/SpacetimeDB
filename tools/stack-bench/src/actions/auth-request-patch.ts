import { createHash } from 'node:crypto';
import type { Page, Request, Route } from 'playwright';
import { inconclusive } from './actor-action-runtime.js';
import { ActionApplicationFailure } from './action-contract.js';
import { browserApplicationBoundary } from './browser-action-executors.js';

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

// Locate values submitted by the real form, not a guessed route or credential key.
// Positional arguments name nothing, so their fields go by the interface's own
// parameter names; a field the interface has no parameter for is reported, not sent.
export function patchAuthRequest(body: unknown, username: string, password: string, patch: AuthRequestPatch,
  parameterNames?: readonly string[]) {
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
    const last = container.at(-1);
    if (last && typeof last === 'object' && !Array.isArray(last)) {
      container[container.length - 1] = { ...last, ...fields };
    } else if (Object.keys(fields).length) {
      if (parameterNames?.length !== container.length) throw new Error('Positional credential fields need parameter names');
      for (const [key, value] of Object.entries(fields)) {
        const index = parameterNames.findIndex(name => sameIdentifier(name, key));
        if (index < 0) absentParameters.push(key);
        else container[index] = value;
      }
    }
  } else for (const [key, value] of Object.entries(fields)) {
    Object.defineProperty(container, key, { value, enumerable: true, writable: true, configurable: true });
  }
  return { body: JSON.stringify(copy), shape: Array.isArray(container) ? 'positional' : 'object',
    ...(absentParameters.length ? { absentParameters } : {}) };
}

// A SpacetimeDB function call sends positional arguments; its module schema
// names them. Read it through the page's own network, as the browser sees it.
async function callParameterNames(request: Request): Promise<string[] | undefined> {
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
    visit(await response.json());
    const names = found.length === 1 ? found[0]!.map(element =>
      (element as { name?: { some?: unknown } } | null)?.name?.some) : [];
    return names.length && names.every(item => typeof item === 'string') ? names as string[] : undefined;
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
    const names = Array.isArray(body) && Object.keys(patch.fields ?? {}).length
      ? await callParameterNames(request) : undefined;
    try { changed = patchAuthRequest(body, username, password, patch, names); }
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
    if (submissionFailure && !(submissionFailure.error instanceof ActionApplicationFailure)) {
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
