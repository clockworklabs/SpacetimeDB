import { createHash } from 'node:crypto';
import type { Page, Route } from 'playwright';
import { inconclusive } from './actor-action-runtime.js';
import { ActionApplicationFailure } from './action-contract.js';
import { browserApplicationBoundary } from './browser-action-executors.js';

export interface AuthRequestPatch {
  readonly fields?: Readonly<Record<string, unknown>>;
  readonly password?: unknown;
}

// Locate values submitted by the real form, not a guessed route or credential key.
export function patchAuthRequest(body: unknown, username: string, password: string, patch: AuthRequestPatch) {
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
  if (Array.isArray(container)) {
    const last = container.at(-1);
    if (last && typeof last === 'object' && !Array.isArray(last)) {
      container[container.length - 1] = { ...last, ...fields };
    } else if (Object.keys(fields).length) container.push({ ...fields });
  } else for (const [key, value] of Object.entries(fields)) {
    Object.defineProperty(container, key, { value, enumerable: true, writable: true, configurable: true });
  }
  return { body: JSON.stringify(copy), shape: Array.isArray(container) ? 'positional' : 'object' };
}

export async function withAuthRequestPatch<T>(page: Pick<Page, 'route' | 'unroute'>,
  username: string, password: string, patch: AuthRequestPatch, submit: () => Promise<T>) {
  let matches = 0, error = false;
  const pending: Promise<void>[] = [];
  let receipt: { shape: string; status: number; bodySha256: string } | undefined;
  const handler = async (route: Route) => {
    const request = route.request();
    let body: unknown;
    try { body = request.postDataJSON(); } catch { return route.fallback(); }
    let changed: ReturnType<typeof patchAuthRequest>;
    try { changed = patchAuthRequest(body, username, password, patch); }
    catch { error = true; return route.abort(); }
    if (!changed) return route.fallback();
    const contentType = request.headers()['content-type']?.split(';')[0]?.trim();
    if (request.method() !== 'POST' || !contentType || !/^application\/(?:[\w.-]+\+)?json$/i.test(contentType)
      || ++matches !== 1) { error = true; return route.abort(); }
    const sent = (async () => {
      try {
        // Preserve the actual route, headers and native envelope. No retries or redirects.
        const response = await route.fetch({ postData: changed.body, maxRedirects: 0, maxRetries: 0, timeout: 30_000 });
        receipt = { shape: changed.shape, status: response.status(),
          bodySha256: createHash('sha256').update(changed.body).digest('hex') };
        await route.fulfill({ response });
      } catch { error = true; await route.abort().catch(() => {}); }
    })();
    pending.push(sent);
    await sent;
  };
  await page.route('**/*', handler);
  try {
    let result: T | undefined, submissionFailure: { error: unknown } | undefined;
    try { result = await browserApplicationBoundary(submit)(undefined); }
    catch (error) { submissionFailure = { error }; }
    await Promise.all(pending);
    if (submissionFailure && !(submissionFailure.error instanceof ActionApplicationFailure)) {
      throw submissionFailure.error;
    }
    if (error || matches !== 1 || !receipt || receipt.status >= 300 && receipt.status < 400) {
      inconclusive('replay-unavailable', { actor: 'authentication form', detail: 'Could not prove one complete modified credential request' });
    }
    if (submissionFailure) throw submissionFailure.error;
    return { ...result, requestPatch: receipt };
  } finally { await page.unroute('**/*', handler); }
}
