import { mkdirSync, rmSync, statSync } from 'node:fs';

const LOCK_STALE_MS = 2 * 60_000;

// Run `work` holding an exclusive lock next to a shared sign-in file, so exactly one
// process renews it at a time. The lock is judged on the real clock: it is a
// directory the operating system timestamps.
export async function withLoginLock<T>(path: string, work: () => Promise<T>, waitMs = 250): Promise<T> {
  const lock = `${path}.stack-bench-lock`;
  const deadline = Date.now() + LOCK_STALE_MS + 30_000;
  while (true) {
    try { mkdirSync(lock); break; }
    catch (error) {
      if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error;
      // A crashed holder leaves its lock; nothing else can clear it.
      try { if (Date.now() - statSync(lock).mtimeMs > LOCK_STALE_MS) { rmSync(lock, { recursive: true, force: true }); continue; } }
      catch { continue; }
      if (Date.now() > deadline) throw new Error('sign-in renewal lock is held too long');
      await new Promise(resolve => setTimeout(resolve, waitMs));
    }
  }
  try { return await work(); }
  finally { rmSync(lock, { recursive: true, force: true }); }
}
