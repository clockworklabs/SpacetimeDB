import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import test from 'node:test';
import { prepareRun, runSetupCatalog, submitPreparedRun } from '../../src/campaigns/run-setup.js';
import { createDashboardServer, type LaunchInput } from '../../dashboard/dashboard-server.js';
import { initialRun, runSetupPage } from '../../dashboard/public/views/run-setup.js';
import { writePlanFixtures } from '../fixtures/dashboard-fixture.js';

// One invariant: only the exact reviewed configuration can dispatch, and retries
// dispatch it once. No fixture invokes a controller, database, or model.
test('setup reviews exact dimensions, rejects changes, and dispatches one durable job', async t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-setup-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  writePlanFixtures(join(root, 'plans'));
  mkdirSync(join(root, 'run-presets'));
  const presetPath = join(root, 'run-presets', 'ecommerce.json');
  const source = readFileSync(join(root, 'plans', 'ecommerce-progression-reference.json'), 'utf8');
  writeFileSync(presetPath, source);
  const catalog = runSetupCatalog(root, {});
  const choices = structuredClone(catalog);
  choices.workloads[0]!.conditions = [
    { id: 'neutral-dev-no-sdk', guidance: 'neutral-dev-no-sdk' }, { id: 'neutral', guidance: 'neutral' },
  ];
  const initial = initialRun(choices)!;
  assert.deepEqual(initial.conditions, ['neutral']);
  assert.match(runSetupPage(choices, initial, null, '', true), /Dev workflow without SDK skills/);
  const request = { ...initialRun(catalog)!, key: 'setup-smoke', level: 1,
    repetitions: 2, parallelism: 6, maxCostUsd: 12, repairs: 0, pauseAfterDepth: null };
  const review = prepareRun(root, request, {});
  assert.equal(review.attempts, 6);
  assert.equal(review.parallelism, 6);
  assert.equal(review.maxCostUsd, 72);
  assert.equal(prepareRun(root, request, {}).reviewId, review.reviewId);
  assert.equal(existsSync(join(root, 'jobs')), false);
  assert.throws(() => prepareRun(root, { ...request, stacks: [...request.stacks, request.stacks[0]] }, {}), /duplicates/);
  assert.throws(() => prepareRun(root, { ...request, level: 99 }, {}), /Target level/);
  assert.throws(() => prepareRun(root, { ...request, parallelism: 0 }, {}));
  assert.throws(() => prepareRun(root, { ...request, workload: '../escape' }, {}));
  assert.throws(() => submitPreparedRun(root, { request: { ...request, maxCostUsd: 24 }, reviewId: review.reviewId }, {}), /Setup changed/);
  const changed = JSON.parse(source); changed.pricing.models['reference-fixture'].input = 1;
  writeFileSync(presetPath, JSON.stringify(changed));
  assert.throws(() => submitPreparedRun(root, review, {}), /Workload changed/);
  writeFileSync(presetPath, source);
  assert.throws(() => prepareRun(root, request, { STACK_BENCH_CONTROLLER_IMAGE: 'different' }), /runtime/);
  assert.match(runSetupPage(catalog, request, review, '', true), /6 attempts/);
  assert.match(runSetupPage(catalog, request, null, '', true), /name="parallelism"/);

  const launches: LaunchInput[] = [];
  const { server } = createDashboardServer({ resultsRoot: root, plansRoot: join(root, 'plans'),
    allowLaunch: true, token: 'test-token', controlSecret: 'operator-secret-12345678901234567890',
    launch(input) { launches.push(input); return Object.assign(new EventEmitter(), { pid: 1 }); } });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  t.after(() => server.close());
  const origin = `http://127.0.0.1:${(server.address() as AddressInfo).port}`;
  const send = (path: string, value: unknown, secret = 'operator-secret-12345678901234567890') =>
    fetch(origin + path, { method: 'POST', headers: { origin, 'content-type': 'application/json',
      'x-stack-bench-token': 'test-token', 'x-stack-bench-control-secret': secret }, body: JSON.stringify(value) });
  assert.equal((await send('/api/runs', review, 'wrong')).status, 403);
  const preparation = await send('/api/runs/prepare', request);
  assert.equal(preparation.status, 200, await preparation.clone().text());
  const acceptedReview = await preparation.json();
  assert.equal(launches.length, 0);
  const first = await send('/api/runs', acceptedReview);
  assert.equal(first.status, 202, await first.clone().text());
  const result = await first.json() as { campaignKey: string; job: { id: string } };
  assert.equal((await send('/api/runs', acceptedReview)).status, 202);
  assert.equal((await send(`/api/jobs/${result.job.id}/start`, {})).status, 202);
  assert.equal(launches.length, 1);
  assert.equal(launches[0]!.command, 'work');
  assert.equal(launches[0]!.jobId, result.job.id);
  const pending = await (await fetch(`${origin}/api/campaigns/${result.campaignKey}`)).json() as { pendingJob: { status: string } };
  assert.equal(pending.pendingJob.status, 'queued');
  assert.equal((await send(`/api/jobs/${result.job.id}/cancel`, {})).status, 202);
  assert.equal((await send('/api/runs', acceptedReview)).status, 202);
  assert.equal(launches.length, 1, 'cancelled work must not restart');
});
