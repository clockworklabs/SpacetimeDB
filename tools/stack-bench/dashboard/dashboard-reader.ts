import { isMainThread, parentPort, Worker } from 'node:worker_threads';
import { overviewPage, campaignLiveSheet, campaignLiveUpdate, campaignLiveProgression,
  attemptChecks, attemptPackage, attemptTranscript, attemptLogSlice } from './dashboard-views.js';
import { discoverPlans } from './dashboard-model.js';

const reads = { overviewPage, campaignLiveSheet, campaignLiveUpdate, campaignLiveProgression,
  attemptChecks, attemptPackage, attemptTranscript, attemptLogSlice, discoverPlans };
type ReadName = keyof typeof reads;

if (!isMainThread) {
  parentPort!.on('message', async ({ id, name, args }: { id: number; name: ReadName; args: never[] }) => {
    try { parentPort!.postMessage({ id, value: await (reads[name] as (...input: never[]) => unknown)(...args) }); }
    catch (error) { parentPort!.postMessage({ id, error: error instanceof Error ? error.message : String(error) }); }
  });
}

// One reader retains the existing view caches. CPU-heavy evidence validation
// must not block health, events, or run controls on the HTTP thread.
export function createDashboardReader() {
  let worker: Worker | null = null;
  let sequence = 0;
  let closed = false;
  const pending = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
  const inflight = new Map<string, Promise<unknown>>();
  function read<Name extends ReadName>(name: Name, ...args: Parameters<typeof reads[Name]>): Promise<Awaited<ReturnType<typeof reads[Name]>>> {
    if (closed) return Promise.reject(new Error('Dashboard reader closed'));
    const key = JSON.stringify([name, args]);
    if (!worker) {
      const active = worker = new Worker(new URL('./dashboard-reader.js', import.meta.url));
      const fail = (error: Error): void => {
        if (worker !== active) return;
        for (const request of pending.values()) request.reject(error);
        pending.clear();
        inflight.clear();
        worker = null;
      };
      active.on('message', ({ id, value, error }) => {
        const request = pending.get(id);
        pending.delete(id);
        if (error !== undefined) request?.reject(new Error(error));
        else request?.resolve(value);
      });
      active.once('error', fail);
      active.once('exit', code => fail(new Error(`Dashboard reader exited (${code})`)));
    }
    let result = inflight.get(key);
    if (!result) {
      const id = ++sequence;
      result = new Promise((resolve, reject) => {
        pending.set(id, { resolve, reject });
        worker!.postMessage({ id, name, args });
      }).finally(() => inflight.delete(key));
      inflight.set(key, result);
    }
    return result as Promise<Awaited<ReturnType<typeof reads[Name]>>>;
  }
  return { read, close: () => { closed = true; void worker?.terminate(); } };
}
