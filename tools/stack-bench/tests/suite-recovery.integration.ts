import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createServer } from 'node:http';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { basename, join } from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { STACK_BENCH_ROOT, compiledEntrypoint } from '../src/package-root.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { readArtifactPayload } from '../src/evidence/artifacts.js';
import type { GradeBundlePayload } from '../src/evidence/benchmark-run.js';
import { classifyBundle } from '../src/evidence/outcomes.js';

// Exercise the actual runner, browser, grader processes, evidence writer and
// outcome consumer. The stub has no mutable database; backend resets need their
// existing native lifecycle checks as well. Keep the receipt outside the fixture.
for (const mode of ['persistent', 'once'] as const) test(`isolated recovery preserves other failures with a ${mode} navigation timeout`, {
  timeout: 180_000,
}, async () => {
  const root = mkdtempSync(join(STACK_BENCH_ROOT, 'tracks', 'recovery-test-'));
  const evidence = process.env.STACK_BENCH_RECOVERY_EVIDENCE_DIR
    ? join(process.env.STACK_BENCH_RECOVERY_EVIDENCE_DIR, mode) : undefined;
  const walkDirectory = join(STACK_BENCH_ROOT, 'dist', 'tracks', basename(root));
  let stallRequests = 0;
  const visits: Record<string, number> = {};
  const server = createServer((request, response) => {
    if (request.url?.startsWith('/record/')) {
      const name = request.url.slice('/record/'.length);
      visits[name] = (visits[name] ?? 0) + 1;
      response.end('ok'); return;
    }
    if (request.url === '/stall.css') return;
    const stall = request.headers.cookie?.includes('stall=1');
    if (stall) stallRequests++;
    response.writeHead(200, { 'Content-Type': 'text/html', 'Set-Cookie': 'stall=; Max-Age=0; Path=/' });
    response.end(`${stall && (mode === 'persistent' || stallRequests === 1) ? '<link rel="stylesheet" href="/stall.css"><script type="module">document.body.dataset.loaded="yes"</script>' : ''}
      <h1 data-role="app-title">Fixture</h1><span data-role="ready">ready</span>
      ${['good', 'bad', 'stall'].map(name => `<button data-role="${name}" onclick="
        ${name === 'stall' ? "document.cookie='stall=1; Path=/';" : ''}
        fetch('/record/${name}');">${name}</button>`).join('')}`);
  });
  const receipt: Record<string, unknown> = { result: 'running',
    command: 'node --test dist/tests/suite-recovery.integration.js',
    note: `${mode} stylesheet stall; two completed suites must not repeat. No model calls.` };
  try {
    mkdirSync(walkDirectory, { recursive: true });
    cpSync(join(STACK_BENCH_ROOT, 'dist', 'tracks', 'loop', 'walk.js'), join(walkDirectory, 'walk.js'));
    mkdirSync(join(root, 'contracts'));
    mkdirSync(join(root, 'scenarios'));
    mkdirSync(join(root, 'app'));
    writeFileSync(join(root, 'app', 'fixture.txt'), 'immutable recovery fixture');
    writeFileSync(join(root, 'contracts', '01-recovery.json'), JSON.stringify({ level: 1,
      hooks: [{ id: 'app-title', element: 'title', stage: 'landing', check: 'visible' }] }));
    writeFileSync(join(root, 'track.json'), JSON.stringify({ schemaVersion: 1, title: 'Recovery fixture',
      slug: 'recovery-test', internal: true, validatedThrough: 1, plannedThrough: 1,
      portOffset: 600, restartProbe: '/', suites: { '1': ['good', 'bad', 'stall'].map(id => ({
        id, inherit: 'none', spec: `scenarios/${id}.json` })) } }));
    for (const name of ['good', 'bad', 'stall']) {
      writeFileSync(join(root, 'scenarios', `${name}.json`), JSON.stringify({ schemaVersion: 1, level: 1,
        features: [{ id: 1, name, actors: ['viewer'], setup: [], criteria: [{ id: name, desc: name, points: 1,
          steps: [{ do: 'click', actor: 'viewer', testid: name, settleMs: 100 },
            ...(name === 'stall' ? [{ do: 'reload', actor: 'viewer', settleMs: 0 }] : []),
            { do: 'expect', actor: 'viewer', testid: name === 'bad' ? 'missing' : 'ready', within: 100 }] }] }] }));
    }
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    const address = server.address(); assert(address && typeof address !== 'string');
    const source = hashAppSource(join(root, 'app'));
    const argv = [compiledEntrypoint('commands', 'run-suite.js'), '--app', join(root, 'app'),
      '--url', `http://127.0.0.1:${address.port}`, '--backend', 'stub', '--label', 'recovery-test',
      '--track', basename(root), '--out', join(root, 'results'), '--source-sha256', source.sha256,
      '--no-media', '--no-reset', '--retry-inconclusive'];
    receipt.source = source.sha256;
    const result = await promisify(execFile)(process.execPath, argv,
      { encoding: 'utf8', timeout: 150_000, maxBuffer: 8 * 1024 * 1024 });
    receipt.stdout = result.stdout;
    const bundle = readArtifactPayload<GradeBundlePayload>(join(root, 'results', 'bundle.json'), { expectedKind: 'grade_bundle' });
    receipt.bundle = bundle;
    assert.deepEqual(visits, { good: 1, bad: 1, stall: 2 });
    assert.equal(stallRequests, 2);
    assert.equal(classifyBundle(bundle).kind, mode === 'persistent' ? 'inconclusive' : 'app_failure');
    assert.equal(bundle.suites?.good?.total, 1);
    assert.equal(bundle.suites?.bad?.total, 0);
    assert.equal(bundle.suites?.lint?.pass, true);
    assert.equal(bundle.suites?.stall?.total, mode === 'persistent' ? 0 : 1);
    assert.equal(bundle.totals?.max, 3);
    assert.deepEqual(Object.keys(bundle.suiteRetries ?? {}), ['stall']);
    const initial = JSON.parse(readFileSync(join(root, 'results', 'suite-retries', 'stall', 'initial', 'grading-stall.json'), 'utf8'));
    assert.equal(initial.payload.features[0].criteria[0].evidence.status, 'inconclusive');
    receipt.result = 'passed';
  } catch (error) {
    if (error && typeof error === 'object') {
      if ('stdout' in error) receipt.stdout = error.stdout;
      if ('stderr' in error) receipt.stderr = error.stderr;
    }
    receipt.result = 'failed'; receipt.error = String(error); throw error;
  } finally {
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    receipt.visits = visits;
    receipt.stallRequests = stallRequests;
    if (evidence) {
      mkdirSync(evidence, { recursive: true });
      // Preserve raw reports and diagnostics, including a failed execution.
      if (existsSync(join(root, 'results'))) cpSync(join(root, 'results'), join(evidence, 'results'), { recursive: true });
      writeFileSync(join(evidence, 'receipt.json'), JSON.stringify(receipt, null, 2));
    }
    rmSync(root, { recursive: true, force: true });
    rmSync(walkDirectory, { recursive: true, force: true });
  }
});
