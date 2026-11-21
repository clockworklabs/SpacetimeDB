import { setTimeout as delay } from 'node:timers/promises';
import { fetchStatus } from '../runtime/readiness.js';

export async function answers(url: string,
  { freshConnection = false, requireSuccess = false }: {
    freshConnection?: boolean; requireSuccess?: boolean;
  } = {}): Promise<boolean> {
  const status = await fetchStatus(url, { timeoutMs: 5000,
    ...(freshConnection ? { init: { headers: { connection: 'close' } } } : {}) });
  return status !== null && (!requireSuccess || (status >= 200 && status < 300));
}

export async function waitFor(check: () => Promise<boolean>, timeoutMs: number,
  description: string, signal?: AbortSignal | null): Promise<void> {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (signal?.aborted) throw signal.reason ?? new Error('backend control cancelled');
    if (await check()) return;
    try { await delay(500, undefined, { signal: signal ?? undefined }); }
    catch (error) {
      throw signal?.aborted ? signal.reason ?? new Error('backend control cancelled') : error;
    }
  }
  throw new Error(`timed out waiting for ${description}`);
}
