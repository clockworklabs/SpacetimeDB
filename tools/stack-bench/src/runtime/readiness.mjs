// A pending Promise does not keep Node alive. Undici can leave a connection
// attempt pending without a referenced handle, which made a readiness process
// reach beforeExit(0) while still awaiting fetch(). Keep an ordinary timer
// referenced, abort the request at the deadline, and race even a non-cooperative
// fetch implementation so every probe has a terminal result.
export async function fetchStatus(url, { timeoutMs = 5000, init = {}, fetchImpl = globalThis.fetch } = {}) {
  if (!Number.isFinite(timeoutMs) || timeoutMs <= 0) throw new Error('fetch timeout must be positive');
  const controller = new AbortController();
  let timer;
  const timedOut = new Promise(resolve => {
    timer = setTimeout(() => {
      controller.abort();
      resolve(null);
    }, timeoutMs);
  });
  const requested = Promise.resolve()
    .then(() => fetchImpl(url, { ...init, signal: controller.signal }))
    .then(response => response.status, () => null);
  try {
    return await Promise.race([requested, timedOut]);
  } finally {
    clearTimeout(timer);
  }
}
