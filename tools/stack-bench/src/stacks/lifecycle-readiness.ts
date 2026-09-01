import { fetchStatus } from '../runtime/readiness.js';

const delay = (ms: number, signal?: AbortSignal | null): Promise<void> =>
  new Promise<void>((resolve, reject) => {
    if (signal?.aborted) { reject(signal.reason ?? new Error('backend control cancelled')); return; }
    const timer = setTimeout(done, ms);
    function done(): void {
      signal?.removeEventListener('abort', cancelled);
      resolve();
    }
    function cancelled(): void {
      clearTimeout(timer);
      signal?.removeEventListener('abort', cancelled);
      reject(signal?.reason ?? new Error('backend control cancelled'));
    }
    signal?.addEventListener('abort', cancelled, { once: true });
  });

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
    await delay(500, signal);
  }
  throw new Error(`timed out waiting for ${description}`);
}
