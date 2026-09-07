import { mkdirSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { compileCampaignFile } from '../../src/campaigns/campaign-compiler.js';
import { releaseCampaignReservation, runCampaignAdmission } from '../../src/campaigns/campaign-admission.js';
import { claimBackendResources, createBackendLease, readBackendLease, releaseResourceLocks }
  from '../../src/runtime/backend-lease.js';
import type { BackendLease } from '../../src/runtime/backend-lease.js';
import type { CampaignReservation } from '../../src/campaigns/campaign-admission.js';
import { STACK_BENCH_ROOT } from '../../src/package-root.js';

const [directory, locks, capacity, racers, standaloneIndex] = process.argv.slice(2);
if (!directory || !locks) process.exit(2);
const plan = compileCampaignFile(join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign.deterministic.json'));
process.send?.('ready');
let reservation: CampaignReservation | undefined;
let standalone: BackendLease | undefined;
let joinedRace = false;
process.on('message', message => {
  if (message === 'release') {
    if (reservation) releaseCampaignReservation(reservation);
    if (standalone) releaseResourceLocks(standalone);
    reservation = undefined;
    standalone = undefined;
    process.send?.('released');
    return;
  }
  try {
    if (standaloneIndex !== undefined) {
      const runIndex = Number(standaloneIndex);
      const port = message && typeof message === 'object' && 'port' in message
        ? Number(message.port) : 20000 + runIndex;
      standalone = createBackendLease({ runId: `standalone-${runIndex}`, backend: 'stub',
        track: 'loop', runIndex });
      claimBackendResources(join(directory, 'standalone.json'), standalone, { root: locks,
        keys: ['capacity:runner:0', `port:${port}`], capacity: Number(capacity) });
      process.send?.({ status: 'admitted', runIndices: [runIndex],
        keys: standalone.resources.locks.map(lock => lock.key) });
      return;
    }
    const result = runCampaignAdmission(plan, directory, { env: { STACK_BENCH_RESOURCE_LOCK_DIR: locks,
      STACK_BENCH_APPLIANCE: '1', ...(capacity ? { STACK_BENCH_RUNNER_CAPACITY: capacity } : {}) },
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
    if (capacity) {
      const lease = readBackendLease(reservation!.path, { token: reservation!.token });
      process.send?.({ status: 'admitted', runIndices: result.runIndices, keys: lease.resources.locks.map(lock => lock.key) });
    } else process.send?.('admitted');
  } catch (error) {
    if (standalone) releaseResourceLocks(standalone);
    standalone = undefined;
    writeFileSync(join(directory, 'admission-error'), String(error));
    process.send?.('refused');
  }
});
