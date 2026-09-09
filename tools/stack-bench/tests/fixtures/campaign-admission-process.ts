import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { compileCampaignFile } from '../../src/campaigns/campaign-compiler.js';
import { releaseCampaignReservation, runCampaignAdmission } from '../../src/campaigns/campaign-admission.js';
import { readBackendLease }
  from '../../src/runtime/backend-lease.js';
import type { CampaignReservation } from '../../src/campaigns/campaign-admission.js';
import { STACK_BENCH_ROOT } from '../../src/package-root.js';

const [directory, locks, workers, racers] = process.argv.slice(2);
if (!directory || !locks) process.exit(2);
const manifest = JSON.parse(readFileSync(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'), 'utf8'));
manifest.parallelism = Number(workers ?? 1);
const manifestPath = join(directory, 'manifest.json');
writeFileSync(manifestPath, JSON.stringify(manifest));
const plan = compileCampaignFile(manifestPath);
process.send?.('ready');
let reservation: CampaignReservation | undefined;
let joinedRace = false;
process.on('message', async message => {
  if (message === 'release') {
    if (reservation) releaseCampaignReservation(reservation);
    reservation = undefined;
    process.send?.('released');
    return;
  }
  try {
    const result = await runCampaignAdmission(plan, directory, { env: { STACK_BENCH_RESOURCE_LOCK_DIR: locks,
      STACK_BENCH_APPLIANCE: '1' },
    probePort: () => {
      if (racers && !joinedRace) {
        joinedRace = true;
        mkdirSync(locks, { recursive: true });
        writeFileSync(join(locks, `racer-${process.pid}`), 'ready');
        const deadline = Date.now() + 10000;
        while (readdirSync(locks).filter(name => name.startsWith('racer-')).length < Number(racers)) {
          if (Date.now() > deadline) throw new Error('claim race barrier timed out');
          Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
        }
      }
      return { free: true };
    },
    preflight: request => {
      writeFileSync(join(directory, 'preflight-entered'), 'entered');
      return { schemaVersion: 1, generatedAt: new Date().toISOString(),
        request: { backends: request.backends, track: request.track, levels: request.levelList,
          runIndex: request.runIndex, parallelism: request.parallelism, agentAdapter: request.agentAdapter,
          packs: request.packIds, checks: request.checkKeys, image: request.image,
          resultsDir: request.resultsDir, smoke: request.smoke },
        ok: true, summary: { passed: 0, failed: 0, warnings: 0 }, checks: [] };
    } });
    reservation = result.reservation;
    {
      const lease = readBackendLease(reservation!.path, { token: reservation!.token });
      process.send?.({ status: 'admitted', runIndices: result.runIndices, keys: lease.resources.locks.map(lock => lock.key) });
    }
  } catch (error) {
    writeFileSync(join(directory, 'admission-error'), String(error));
    process.send?.('refused');
  }
});
