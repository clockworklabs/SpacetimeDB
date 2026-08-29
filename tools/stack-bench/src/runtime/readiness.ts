// A pending Promise does not keep Node alive. Undici can leave a connection
// attempt pending without a referenced handle, which made a readiness process
// reach beforeExit(0) while still awaiting fetch(). Keep an ordinary timer
// referenced, abort the request at the deadline, and race even a non-cooperative
// fetch implementation so every probe has a terminal result.
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
