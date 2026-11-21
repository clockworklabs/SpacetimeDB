import { acquireCampaignLock, releaseCampaignLock, watchCampaignCancellation }
  from '../../src/campaigns/campaign-lock.js';
const [root] = process.argv.slice(2);
try {
  const lock = acquireCampaignLock(root!, { id: 'native-lock-proof', contentSha256: 'a'.repeat(64) });
  process.stdout.write('acquired\n');
  {
    const watcher = watchCampaignCancellation(lock);
    const deadline = setTimeout(() => { throw new Error('cancellation was not received'); }, 10_000);
    try { await new Promise<void>(resolve => watcher.signal.addEventListener('abort', () => resolve(), { once: true })); }
    finally { clearTimeout(deadline); watcher.close(); }
  }
  releaseCampaignLock(lock);
} catch (error) {
  process.stderr.write(error instanceof Error ? error.message : String(error));
  process.exitCode = 2;
}
