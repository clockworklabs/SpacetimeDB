// Keep a referenced timer so every readiness probe reaches a terminal result.
type FetchStatusImplementation = (
  url: string,
  init: RequestInit,
) => PromiseLike<{ status: number }> | { status: number };

interface FetchStatusOptions {
  fetchImpl?: FetchStatusImplementation;
  init?: RequestInit;
  timeoutMs?: number;
}

export async function fetchStatus(
  url: string,
  { timeoutMs = 5000, init = {}, fetchImpl = globalThis.fetch }: FetchStatusOptions = {},
): Promise<number | null> {
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new Error('fetch timeout must be positive');
  const controller = new AbortController();
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timedOut = new Promise<null>(resolve => {
    timer = setTimeout(() => {
      controller.abort();
      resolve(null);
    }, timeoutMs);
  });
  const requested = Promise.resolve()
    .then(() => fetchImpl(url, { ...init, signal: controller.signal }))
    .then(response => response.status, (): null => null);
  try {
    return await Promise.race([requested, timedOut]);
  } finally {
    clearTimeout(timer);
  }
}
