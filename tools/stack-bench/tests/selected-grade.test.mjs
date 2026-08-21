import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { readArtifactPayload } from '../src/evidence/artifacts.mjs';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { resolveRecipeSelection } from '../src/composition/recipe-selection.mjs';
import { loadTrack } from '../src/composition/tracks.mjs';

const GRADER = join(import.meta.dirname, '..', 'grader', 'grade.mjs');

function startBlankApp(html = '<!doctype html><html><body></body></html>') {
  const source = `
    import { createServer } from 'node:http';
    const server = createServer((request, response) => {
      response.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
      response.end(${JSON.stringify(html)});
    });
    server.listen(0, '127.0.0.1', () => console.log(server.address().port));
    process.on('SIGTERM', () => server.close(() => process.exit(0)));
  `;
  const child = spawn(process.execPath, ['--input-type=module', '--eval', source], {
    stdio: ['ignore', 'pipe', 'pipe'], windowsHide: true,
  });
  const port = new Promise((resolve, reject) => {
    const deadline = setTimeout(() => reject(new Error('blank app did not start')), 10_000);
    child.once('error', reject);
    child.stdout.once('data', data => {
      clearTimeout(deadline);
      resolve(Number(data.toString().trim()));
    });
  });
  return { child, port };
}

function run(file, argv) {
  return new Promise((resolve, reject) => {
    execFile(process.execPath, [file, ...argv], {
      encoding: 'utf8', timeout: 60_000, maxBuffer: 8 * 1024 * 1024,
    }, (error, stdout, stderr) => {
      if (error) reject(new Error(`${error.message}\n${stdout}\n${stderr}`));
      else resolve({ stdout, stderr });
    });
  });
}

test('the live grader executes and reports exactly one selected stable check', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-selected-grade-'));
  const app = join(root, 'app');
  const out = join(root, 'grade.json');
  mkdirSync(app, { recursive: true });
  const server = startBlankApp();
  try {
    const port = await server.port;
    const track = loadTrack('ecommerce');
    const binding = resolveRecipeRelease(track, 1);
    const check = binding.release.checkCatalog.find(candidate => candidate.criterionId === '2a');
    const selection = resolveRecipeSelection(binding.release, { checkKeys: [check.stableKey] });
    await run(GRADER, [
      '--url', `http://127.0.0.1:${port}`,
      '--level', '1', '--track', 'ecommerce', '--backend', 'postgres', '--app', app,
      '--spec', join(track.dir, check.source), '--out', out,
      '--expected-recipe-sha256', binding.release.contentSha256,
      '--selected-check', check.stableKey, '--selection-sha256', selection.sha256,
    ]);
    const report = readArtifactPayload(out, { expectedKind: 'grade' });
    assert.equal(report.selection.sha256, selection.sha256);
    assert.deepEqual(report.selection.checks.map(item => item.stableKey), [check.stableKey]);
    assert.equal(report.features.length, 1);
    assert.deepEqual(report.features[0].criteria.map(item => item.stableKey), [check.stableKey]);
    assert.equal(report.features[0].setupEvidence.status, 'failed');
    assert.equal(report.features[0].criteria[0].evidence.status, 'failed');
    assert.equal(report.features[0].criteria[0].evidence.phase, 'setup');
    assert.deepEqual(report.features[0].criteria[0].evidence.actions, []);
    assert.deepEqual(report.features[0].criteria[0].evidence.attachments,
      [{ kind: 'check-evidence', ref: 'feature.setupEvidence' }]);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});

test('the live grader executes account setup through the registered actor executor', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-actor-grade-'));
  const out = join(root, 'grade.json');
  const spec = join(root, 'scenario.json');
  writeFileSync(spec, JSON.stringify({
    level: 1,
    features: [{
      id: 1,
      name: 'account setup',
      actors: ['a'],
      setup: [{ do: 'signUp', actor: 'a', name: 'Alice' }],
      criteria: [{ id: '1a', desc: 'signed-in identity is visible',
        steps: [{ do: 'expect', actor: 'a', testid: 'current-user' }] }],
    }],
  }));
  const server = startBlankApp(`<!doctype html><html><body>
    <input data-testid="signup-username"><input data-testid="signup-password">
    <button data-testid="signup-submit">Sign up</button>
    <div data-testid="current-user" hidden></div>
    <script>
      document.querySelector('[data-testid="signup-submit"]').onclick = () => {
        const current = document.querySelector('[data-testid="current-user"]');
        current.textContent = document.querySelector('[data-testid="signup-username"]').value;
        current.hidden = false;
      };
    </script>
  </body></html>`);
  try {
    const port = await server.port;
    await run(GRADER, ['--url', `http://127.0.0.1:${port}`, '--level', '1',
      '--spec', spec, '--out', out]);
    const report = readArtifactPayload(out, { expectedKind: 'grade' });
    assert.equal(report.total, 1);
    assert.equal(report.max, 1);
    assert.equal(report.features[0].criteria[0].evidence.status, 'passed');
    assert.equal(report.features[0].setupEvidence.status, 'passed');
    assert.deepEqual(report.features[0].setupEvidence.actions.map(item => item.evidence.action.id), ['signUp']);
    assert.equal(report.features[0].criteria[0].evidence.status, 'passed');
    assert.deepEqual(report.features[0].criteria[0].evidence.actions.map(item => item.evidence.action.id), ['expect']);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});

test('an inconclusive check keeps the recipe denominator fixed', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-fixed-denominator-'));
  const app = join(root, 'app');
  const out = join(root, 'grade.json');
  const spec = join(root, 'scenario.json');
  mkdirSync(app, { recursive: true });
  writeFileSync(spec, JSON.stringify({
    level: 1,
    features: [{
      id: 1,
      name: 'unsupported action',
      actors: ['a'],
      setup: [],
      criteria: [{ id: '1a', desc: 'the declared point remains in the contract', points: 3,
        steps: [{ do: 'callAction', actor: 'a', action: 'not-declared',
          input: { testid: 'action-input', attribute: 'data-input' } }] }],
    }],
  }));
  const server = startBlankApp('<!doctype html><div data-testid="action-input" data-input="{}"></div>');
  try {
    const port = await server.port;
    await run(GRADER, ['--url', `http://127.0.0.1:${port}`, '--level', '1',
      '--backend', 'postgres', '--app', app, '--spec', spec, '--out', out]);
    const report = readArtifactPayload(out, { expectedKind: 'grade' });
    assert.equal(report.total, 0);
    assert.equal(report.max, 3);
    assert.equal(report.features[0].max, 3);
    assert.equal(report.features[0].criteria[0].evidence.status, 'inconclusive');
    assert.equal(report.inconclusive[0].points, 3);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});
