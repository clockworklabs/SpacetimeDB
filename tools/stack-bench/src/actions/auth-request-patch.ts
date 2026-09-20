import { createHash } from 'node:crypto';
import type { Page, Route } from 'playwright';
import { inconclusive } from './actor-action-runtime.js';

export interface AuthRequestPatch {
  readonly fields: Readonly<Record<string, unknown>>;
}

// Locate values submitted by the real form, not a guessed route or credential key.
export function patchAuthRequest(body: unknown, username: string, password: string, patch: AuthRequestPatch) {
  if (Object.keys(patch).some(key => key !== 'fields') || !patch.fields || Array.isArray(patch.fields)
    || typeof patch.fields !== 'object' || !Object.keys(patch.fields).length) throw new Error('Expected nonempty authentication fields');
  const copy = structuredClone(body), matches: (Record<string, unknown> | unknown[])[] = [];
  const visit = (value: unknown): void => {
    if (!value || typeof value !== 'object') return;
    const entries = Object.entries(value);
    const users = entries.filter(([, v]) => v === username), secrets = entries.filter(([, v]) => v === password);
    if (users.length && secrets.length) {
      if (users.length !== 1 || secrets.length !== 1 || users[0]![0] === secrets[0]![0]) {
        throw new Error('Ambiguous credential values');
      }
      matches.push(value as Record<string, unknown>);
    }
    for (const [, child] of entries) visit(child);
  };
  visit(copy);
  if (!matches.length) return null;
  if (matches.length !== 1) throw new Error('Multiple credential containers');
  const container = matches[0]!;
  if (Array.isArray(container)) {
    const last = container.at(-1);
    if (last && typeof last === 'object' && !Array.isArray(last)) {
      container[container.length - 1] = { ...last, ...patch.fields };
    } else container.push({ ...patch.fields });
  } else for (const [key, value] of Object.entries(patch.fields)) {
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
    let result: T | undefined, submissionError: unknown;
    try { result = await submit(); } catch (caught) { submissionError = caught; }
    await Promise.all(pending);
    if (error || matches !== 1 || !receipt || receipt.status >= 300 && receipt.status < 400) {
      inconclusive('replay-unavailable', { actor: 'authentication form', detail: 'Could not prove one complete modified credential request' });
    }
    if (submissionError) throw submissionError;
    return { ...result, requestPatch: receipt };
  } finally { await page.unroute('**/*', handler); }
}
