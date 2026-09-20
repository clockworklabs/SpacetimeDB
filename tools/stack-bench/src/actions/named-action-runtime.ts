import { fail, inconclusive } from './actor-action-runtime.js';
import type { Actor, HeaderRecord } from './actor-action-runtime.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import type { NamedAction } from '../composition/tracks.js';
import type { SpacetimeTarget } from '../stacks/stack-grading-operations.js';
import { classifyConvexFunctionResponse } from '../stacks/backends/convex-protocol.js';
import { leaseFromEnv } from '../runtime/backend-lease.js';
import { evidenceNowMs } from '../evidence/evidence-timing.js';

interface StorageLike {
  readonly length: number;
  getItem(key: string): string | null;
  key(index: number): string | null;
}

declare const localStorage: StorageLike;
declare const sessionStorage: StorageLike;
declare const window: { getSessionToken?: () => unknown };

export type { NamedAction } from '../composition/tracks.js';

export type ResponseContract = 'http' | 'spacetime-reducer' | 'convex-mutation' | 'convex-query' | 'convex-action';
export type RefusalKind = 'access' | 'validation' | 'application' | 'not-found' | null;
export interface NamedActionResponse {
  readonly ok: boolean;
  readonly applicationRejected: boolean;
  readonly refusalKind: RefusalKind;
  readonly responseContract: ResponseContract;
  readonly complete: boolean;
}

export interface NamedActionRequest {
  readonly responseContract?: ResponseContract;
  readonly applicationRejectionStatuses?: readonly number[];
  readonly body?: string | null;
  readonly method?: string;
  readonly url?: string | null;
}

export interface ConcurrentCallOutcome {
  readonly action?: string;
  readonly values?: Readonly<Record<string, unknown>>;
  readonly requestIndex?: number;
  readonly startedAtMs?: number;
  readonly completedAtMs?: number;
  readonly durationMs?: number;
  readonly transport?: 'response' | 'error' | 'timeout' | 'cancelled';
  readonly applicationRejected?: boolean;
  readonly refusalKind?: RefusalKind;
  readonly responseContract?: ResponseContract;
  readonly complete?: boolean;
  readonly name: string;
  readonly ok: boolean;
  readonly status: number;
  readonly text: string;
}

export interface ConcurrentCallResult {
  readonly action: string;
  readonly fired: number;
  readonly ms: number;
  readonly outcomes: readonly ConcurrentCallOutcome[];
}

export interface NamedActionsCapability {
  readonly spacetime?: SpacetimeTarget | null;
  classifyResponse?(request: Omit<NamedActionRequest, 'body'>, response: {status: number; text: string}): NamedActionResponse;
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

// Only complete response bodies enter this classifier. Network/body failures remain unknown.
export function classifyNamedActionResponse(named: Pick<NamedActionsCapability, 'classifyResponse'>,
  request: Omit<NamedActionRequest, 'body'>, response: { status: number; text: string }): NamedActionResponse {
  if (named.classifyResponse) return named.classifyResponse(request, response);
  return classifyResponseContract(request, response);
}

export function classifyResponseContract(request: Omit<NamedActionRequest, 'body'>,
  response: { status: number; text: string }): NamedActionResponse {
  const responseContract = request.responseContract ?? (request.applicationRejectionStatuses ? 'spacetime-reducer' : 'http');
  if (responseContract.startsWith('convex-')) {
    const result = classifyConvexFunctionResponse(response.status, response.text);
    return { ok: result.kind === 'accepted', applicationRejected: result.kind === 'application-error',
      refusalKind: [401, 403].includes(response.status) ? 'access'
        : result.kind === 'application-error' ? 'application' : result.kind === 'validation-error' ? 'validation' : null,
      responseContract, complete: response.status !== 0 };
  }
  const applicationRejected = (request.applicationRejectionStatuses ?? []).includes(response.status);
  return { ok: response.status >= 200 && response.status < 300, applicationRejected,
    refusalKind: applicationRejected ? 'application' : [401, 403].includes(response.status) ? 'access'
      : [400, 409, 422].includes(response.status) ? 'validation' : response.status === 404 ? 'not-found' : null,
    responseContract, complete: response.status !== 0 };
}

function errorField(error: unknown, field: string): unknown {
  return typeof error === 'object' && error !== null
    ? (error as Record<string, unknown>)[field]
    : undefined;
}

export function namedActionRequest(named: NamedActionsCapability, action: NamedAction,
  input: unknown): NamedActionRequest | null {
  try { return named.request(action, input); }
  catch (error) {
    if (errorField(error, 'code') === 'invalid_named_action_input') {
      fail('interface-invalid', { action: action.id ?? '', attribute: 'input',
        detail: String(errorField(error, 'message') ?? 'invalid named action input') });
    }
    throw error;
  }
}

const AUTH_HEADER = /^(authorization|x-auth-token|x-session|x-token|x-user)$/i;
export const REQUEST_CONTEXT_HEADER = /^(authorization|cookie|x-auth-token|x-session|x-token|x-user|x-csrf-token|x-xsrf-token|csrf-token|origin|referer)$/i;

// Do not strip secondary credentials or alter CSRF state to force a result.
export function tamperedSessionCredentials(headers: HeaderRecord, actor: string): HeaderRecord {
  const auth = Object.entries(headers).filter(([key]) => AUTH_HEADER.test(key) || /^cookie$/i.test(key));
  const [key, value] = auth[0] ?? [];
  const bearer = key?.toLowerCase() === 'authorization' && /^Bearer [A-Za-z0-9._~-]+$/i.test(value ?? '');
  const cookie = key?.toLowerCase() === 'cookie' && /^[A-Za-z0-9_-]+=[A-Za-z0-9._~-]+$/.test(value ?? '')
    && !Object.keys(headers).some(name => /csrf|xsrf/i.test(name)) && !/csrf|xsrf/i.test(value!.split('=')[0]!);
  if (auth.length !== 1 || (!bearer && !cookie)) {
    inconclusive('replay-unavailable', { actor, detail: 'tampering requires one bearer token or one session cookie without CSRF ambiguity' });
  }
  const prefix = bearer ? 7 : value!.indexOf('=') + 1;
  const token = value!.slice(prefix), parts = token.split('.');
  const offset = parts.length === 3 && parts.every(part => /^[A-Za-z0-9_-]+$/.test(part))
    ? parts[0]!.length + parts[1]!.length + 2 : 0;
  // Change the first signature character of a JWT, not its padding bits or claims.
  const original = token[offset]!;
  const replacement = /[0-9]/.test(original) ? (original === '0' ? '1' : '0')
    : /[a-z]/.test(original) ? (original === 'a' ? 'b' : 'a') : (original === 'A' ? 'B' : 'A');
  return { ...headers, [key!]: `${value!.slice(0, prefix)}${token.slice(0, offset)}${replacement}${token.slice(offset + 1)}` };
}

function capturedCredentials(actor: Actor, targetUrl: string): HeaderRecord {
  const target = new URL(targetUrl);
  for (const write of [...(actor.writes ?? [])].reverse()) {
    if (new URL(write.url).origin !== target.origin) continue;
    const headers = Object.entries(write.headers ?? {})
      // Cookies come from the current browser jar, with domain/path matching.
      .filter(([key, value]) => key.toLowerCase() !== 'cookie' && REQUEST_CONTEXT_HEADER.test(key) && value)
      .map(([key, value]) => [key.toLowerCase(), value]);
    if (headers.length) return Object.fromEntries(headers);
  }
  return {};
}

export async function browserCredentials(actor: Actor, targetUrl: string, allowAnonymous = false): Promise<HeaderRecord | null> {
  const headers = capturedCredentials(actor, targetUrl);
  const cookies = await actor.context.cookies(targetUrl);
  if (cookies.length) headers.Cookie = cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ');
  const hasCapturedAuth = Object.keys(headers).some(key => AUTH_HEADER.test(key));
  const tokens = await actor.page.evaluate(() => {
    try {
      const getToken = typeof window === 'undefined' ? undefined : window.getSessionToken;
      if (getToken !== undefined) {
        if (typeof getToken !== 'function') return { unavailable: 'getSessionToken is not a function' };
        const token = getToken.call(window);
        if (token === null) return { signedOut: true };
        return typeof token === 'string' && token.trim() && !/[\r\n]/.test(token) ? { currentToken: token }
          : { unavailable: 'getSessionToken did not return a nonempty, valid bearer token or null' };
      }
    } catch { return { unavailable: 'getSessionToken could not be read' }; }
    const credentialKey = /(?:^|[_-])(auth|jwt|session|token)(?:$|[_-])|(?:auth|jwt|session|token)$/i;
    const excludedKey = /(?:csrf|refresh)/i;
    const usable = (value: unknown): value is string =>
      typeof value === 'string' && value.trim().length >= 8;
    const found = new Set<string>();
    const fromObject = (value: unknown): void => {
      if (!value || typeof value !== 'object') return;
      for (const [key, nested] of Object.entries(value)) {
        if (!excludedKey.test(key) && credentialKey.test(key) && usable(nested)) {
          found.add(nested.trim());
        }
      }
    };
    for (const storage of [localStorage, sessionStorage]) {
      for (let index = 0; index < storage.length; index++) {
        const key = storage.key(index) ?? '';
        const value = storage.getItem(key) ?? '';
        if (!excludedKey.test(key) && credentialKey.test(key) && usable(value)) {
          found.add(value.trim());
        }
        try { fromObject(JSON.parse(value)); } catch { /* not JSON */ }
      }
    }
    return [...found];
  });
  // A live bearer hook supersedes captured credentials after logout or account changes.
  if (tokens && !Array.isArray(tokens)) {
    for (const key of Object.keys(headers)) if (AUTH_HEADER.test(key)) delete headers[key];
    if ('currentToken' in tokens) {
      headers.Authorization = `Bearer ${tokens.currentToken}`;
      return headers;
    }
  } else if (hasCapturedAuth) return headers;
  if (tokens && !Array.isArray(tokens) && 'signedOut' in tokens) {
    if (!cookies.length && !allowAnonymous) {
      inconclusive('replay-unavailable', { actor: actor.name, detail: 'getSessionToken returned no active session token' });
    }
    return cookies.length || allowAnonymous ? headers : null;
  }
  if (tokens && !Array.isArray(tokens) && !cookies.length) {
    inconclusive('replay-unavailable', { actor: actor.name, detail: tokens.unavailable });
  }
  if (allowAnonymous && Array.isArray(tokens) && tokens.length > 1 && !cookies.length) {
    inconclusive('replay-unavailable', { actor: actor.name, detail: 'multiple session tokens without an unambiguous credential hook' });
  }
  if (Array.isArray(tokens) && tokens.length === 1) headers.Authorization = `Bearer ${tokens[0]}`;
  return allowAnonymous || cookies.length || Object.keys(headers).some(key => AUTH_HEADER.test(key)) ? headers : null;
}

type NamedFetch = NamedActionsCapability['fetch'];
const defaultFetch: NamedFetch = (url, options) => fetch(url, options);

export function createNamedActionsCapability({
  actions,
  backend,
  url,
  spacetime,
  lastCalls,
  sleep,
  fetchImpl = defaultFetch,
  now = evidenceNowMs,
}: {
  readonly actions?: readonly NamedAction[];
  readonly backend: string;
  readonly url?: string | null;
  readonly spacetime?: SpacetimeTarget | null;
  readonly lastCalls: {
    get(): ConcurrentCallResult | null;
    set(result: ConcurrentCallResult): void;
  };
  readonly sleep: NamedActionsCapability['sleep'];
  readonly fetchImpl?: NamedFetch;
  readonly now?: () => number;
}): NamedActionsCapability {
  const nativeOrigin = backend === 'convex'
    ? new URL(leaseFromEnv(process.env, { backend: 'convex', active: true }).lease.resources.serverUri!).origin : null;
  return Object.freeze({
    classifyResponse(request: Omit<NamedActionRequest, 'body'>, response: { status: number; text: string }) {
      let responseContract = request.responseContract;
      if (nativeOrigin && request.url) {
        const endpoint = new URL(request.url);
        const kind = /^\/api\/(mutation|query|action)$/.exec(endpoint.pathname)?.[1];
        if (endpoint.origin === nativeOrigin && kind && (request.method ?? 'POST').toUpperCase() === 'POST') {
          responseContract = `convex-${kind}` as ResponseContract;
        }
      }
      return classifyResponseContract({ ...request, ...(responseContract ? { responseContract } : {}) }, response);
    },
    spacetime,
    resolve: (id: string) => (actions ?? []).find(action => action.id === id) ?? null,
    request(action: NamedAction, input: unknown) {
      return STACK_ADAPTER_REGISTRY.get(backend).namedAction.request(
        { action, input, spacetime, url }) as NamedActionRequest | null;
    },
    fetch: fetchImpl,
    lastCalls: Object.freeze({ get: lastCalls.get, set: lastCalls.set }),
    now,
    sleep,
  });
}
