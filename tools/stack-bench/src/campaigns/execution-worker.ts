import { setTimeout as delay } from 'node:timers/promises';
import { listExecutionJobs, workExecutionJob } from './execution-jobs.js';

/** Standalone dispatcher. Stopping it drains claimed work; cancel jobs explicitly. */
export async function runExecutionWorker(results: string, hostId: string, {
  concurrency, signal, env = process.env, work = workExecutionJob,
}: {
  concurrency: number; signal: AbortSignal; env?: NodeJS.ProcessEnv;
  work?: typeof workExecutionJob;
}): Promise<void> {
  if (!Number.isSafeInteger(concurrency) || concurrency < 1) {
    throw new Error('worker concurrency must be a positive safe integer');
  }
  if (!/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(hostId)) throw new Error('worker host ID is invalid');
  const active = new Map<string, Promise<void>>();
  let failure: unknown;
  let after = '';
  try {
    while (!signal.aborted && !failure) {
      // ponytail: directory scans suit the local appliance; use the product queue for a large shared backlog.
      const page = listExecutionJobs(results, { after });
      let scannedPage = true;
      for (const entry of page.jobs) {
        if (signal.aborted || failure) break;
        if (active.size >= concurrency) { scannedPage = false; break; }
        after = entry.job.id;
        if (entry.status !== 'queued' || (entry.job.hostId && entry.job.hostId !== hostId)
          || active.has(entry.job.id)) continue;
        const id = entry.job.id;
        const pending = work(results, id, hostId, { env }).then(() => {}, error => {
          failure = error instanceof Error ? error : new Error(String(error));
        }).finally(() => { active.delete(id); });
        active.set(id, pending);
      }
      if (scannedPage && !page.next) after = '';
      if (!scannedPage || !page.next) {
        // Polling avoids accumulating promise handlers on hours-long active jobs.
        await delay(1_000, undefined, { signal })
          .catch(error => { if (error?.name !== 'AbortError') throw error; });
      }
    }
  } finally { await Promise.all(active.values()); }
  if (failure) throw failure;
}
