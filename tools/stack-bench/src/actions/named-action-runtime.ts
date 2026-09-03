import { fail } from './actor-action-runtime.js';
import type { Actor, HeaderRecord } from './actor-action-runtime.js';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.js';
import type { NamedAction } from '../composition/tracks.js';
import type { SpacetimeTarget } from '../stacks/stack-grading-operations.js';

interface StorageLike {
  readonly length: number;
  getItem(key: string): string | null;
  key(index: number): string | null;
}

declare const localStorage: StorageLike;
declare const sessionStorage: StorageLike;

export type { NamedAction } from '../composition/tracks.js';

export interface NamedActionRequest {
  readonly applicationRejectionStatuses?: readonly number[];
  readonly body?: string | null;
  readonly method?: string;
  readonly url?: string | null;
}

export interface ConcurrentCallOutcome {
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

export function capturedCredentials(actor: Actor): HeaderRecord | null {
  const authHeader = /^(authorization|cookie|x-auth-token|x-session|x-token|x-user)$/i;
  for (const write of [...(actor.writes ?? [])].reverse()) {
    const headers = Object.entries(write.headers ?? {})
      .filter(([key, value]) => authHeader.test(key) && value)
      .map(([key, value]) => [key.toLowerCase(), value]);
    if (headers.length) return Object.fromEntries(headers);
  }
  return null;
}

export async function browserCredentials(actor: Actor): Promise<HeaderRecord | null> {
  const headers: HeaderRecord = {};
  const cookies = await actor.context.cookies();
  if (cookies.length) headers.Cookie = cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ');
  const tokens = await actor.page.evaluate(() => {
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
  if (Array.isArray(tokens) && tokens.length === 1) headers.Authorization = `Bearer ${tokens[0]}`;
  return Object.keys(headers).length ? headers : null;
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
  now = () => Date.now(),
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
  return Object.freeze({
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
