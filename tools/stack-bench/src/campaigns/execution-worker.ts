import { setTimeout as delay } from 'node:timers/promises';
import { listExecutionJobs, workExecutionJob } from './execution-jobs.js';
import { redactCredentials } from '../evidence/diagnostic-sanitizer.js';

/** Standalone dispatcher. Stopping it drains claimed work; cancel jobs explicitly. */
export async function runExecutionWorker(results: string, hostId: string, {
  concurrency, signal, env = process.env, work = workExecutionJob, report = console.error,
}: {
  concurrency: number; signal: AbortSignal; env?: NodeJS.ProcessEnv;
  work?: typeof workExecutionJob;
  report?: (message: string) => void;
}): Promise<void> {
  if (!Number.isSafeInteger(concurrency) || concurrency < 1) {
    throw new Error('worker concurrency must be a positive safe integer');
  }
  if (!/^[a-z0-9][a-z0-9_.-]{0,127}$/.test(hostId)) throw new Error('worker host ID is invalid');
  const active = new Map<string, Promise<void>>();
  const failed = new Set<string>();
  let scanFailures = 0;
  let after = '';
  try {
    while (!signal.aborted) {
      // ponytail: directory scans suit the local appliance; use the product queue for a large shared backlog.
      let page: ReturnType<typeof listExecutionJobs>;
      try { page = listExecutionJobs(results, { after }); scanFailures = 0; }
      catch (error) {
        report(`worker scan failed: ${redactCredentials(String(error))}`);
        if (++scanFailures >= 3) throw error;
        await delay(1_000, undefined, { signal }).catch(error => { if (error?.name !== 'AbortError') throw error; });
        continue;
      }
      for (const entry of page.errors) {
        if (!failed.has(entry.id)) report(`job ${entry.id} cannot be read: ${entry.error}`);
        failed.add(entry.id);
      }
      let scannedPage = true;
      for (const entry of page.jobs) {
        if (signal.aborted) break;
        if (active.size >= concurrency) { scannedPage = false; break; }
        after = entry.job.id;
        if (entry.status !== 'queued' || (entry.job.hostId && entry.job.hostId !== hostId)
          || active.has(entry.job.id) || failed.has(entry.job.id)) continue;
        const id = entry.job.id;
        const pending = work(results, id, hostId, { env }).then(() => {}, error => {
          // Do not fabricate a result or reclaim possibly live children after a worker fault.
          failed.add(id);
          report(`job ${id} requires recovery: ${redactCredentials(String(error))}`);
        }).finally(() => { active.delete(id); });
        active.set(id, pending);
      }
      if (scannedPage) after = page.next ?? '';
      if (!scannedPage || !page.next) {
        // Polling avoids accumulating promise handlers on hours-long active jobs.
        await delay(1_000, undefined, { signal })
          .catch(error => { if (error?.name !== 'AbortError') throw error; });
      }
    }
  } finally { await Promise.all(active.values()); }
}
