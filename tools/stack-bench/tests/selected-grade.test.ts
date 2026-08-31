import assert from 'node:assert/strict';
import { execFile, spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { readGradeArtifactPayload } from '../src/evidence/artifacts.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { resolveRecipeSelection } from '../src/composition/recipe-selection.js';
import { loadTrack } from '../src/composition/tracks.js';
import { compiledEntrypoint } from '../src/package-root.js';

const GRADER = compiledEntrypoint('grader', 'grade.js');

const first = <Value>(values: readonly Value[]): Value => {
  const value = values[0];
  assert(value);
  return value;
};
const record = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const actionIds = (actions: ReadonlyArray<{ evidence: unknown }>): string[] => actions.map(entry => {
  if (!record(entry.evidence) || !record(entry.evidence.action)
    || typeof entry.evidence.action.id !== 'string') {
    throw new Error('grade action evidence has no action id');
  }
  return entry.evidence.action.id;
});

function startBlankApp(html: string = '<!doctype html><html><body></body></html>') {
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
  const port = new Promise<number>((resolve, reject) => {
    const deadline = setTimeout(() => reject(new Error('blank app did not start')), 10_000);
    child.once('error', reject);
    child.stdout.once('data', data => {
      clearTimeout(deadline);
      resolve(Number(data.toString().trim()));
    });
  });
  return { child, port };
}

function run(file: string, argv: readonly string[]): Promise<{ stdout: string; stderr: string }> {
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
    assert(check);
    assert(check.source);
    const selection = resolveRecipeSelection(binding.release, { checkKeys: [check.stableKey] });
    await run(GRADER, [
      '--url', `http://127.0.0.1:${port}`,
      '--level', '1', '--track', 'ecommerce', '--backend', 'postgres', '--app', app,
      '--spec', join(track.dir, check.source), '--out', out,
      '--expected-recipe-sha256', binding.release.contentSha256,
      '--selected-check', check.stableKey, '--selection-sha256', selection.sha256,
    ]);
    const report = readGradeArtifactPayload(out);
    assert(report.selection);
    assert.equal(report.selection.sha256, selection.sha256);
    assert.deepEqual(report.selection.checks.map(item => item.stableKey), [check.stableKey]);
    assert.equal(report.features.length, 1);
    const feature = first(report.features);
    const criterion = first(feature.criteria);
    assert.deepEqual(feature.criteria.map(item => item.stableKey), [check.stableKey]);
    assert.equal(feature.setupEvidence.status, 'failed');
    assert.equal(criterion.evidence.status, 'failed');
    assert.equal(criterion.evidence.phase, 'setup');
    assert.deepEqual(criterion.evidence.actions, []);
    assert.deepEqual(criterion.evidence.attachments,
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
    const report = readGradeArtifactPayload(out);
    assert.equal(report.total, 1);
    assert.equal(report.max, 1);
    const feature = first(report.features);
    const criterion = first(feature.criteria);
    assert.equal(criterion.evidence.status, 'passed');
    assert.equal(feature.setupEvidence.status, 'passed');
    assert.deepEqual(actionIds(feature.setupEvidence.actions), ['signUp']);
    assert.equal(criterion.evidence.status, 'passed');
    assert.deepEqual(actionIds(criterion.evidence.actions), ['expect']);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});

test('setup can wait for app readiness without relaxing scored checks', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-ready-grade-'));
  const out = join(root, 'grade.json');
  const spec = join(root, 'scenario.json');
  writeFileSync(spec, JSON.stringify({
    level: 1,
    features: [{
      id: 1,
      name: 'delayed app readiness',
      actors: ['a'],
      setup: [{ do: 'signUp', actor: 'a', name: 'Alice' }],
      criteria: [
        { id: '1a', desc: 'setup completed', points: 1,
          steps: [{ do: 'expect', actor: 'a', testid: 'current-user' }] },
        { id: '1b', desc: 'scored checks keep the normal deadline', points: 1,
          steps: [
            { do: 'click', actor: 'a', testid: 'slow-action' },
            { do: 'expect', actor: 'a', testid: 'slow-result' },
          ] },
      ],
    }],
  }));
  const server = startBlankApp(`<!doctype html><html><body>
    <div id="app"></div>
    <script>
      setTimeout(() => {
        document.querySelector('#app').innerHTML = \`
          <input data-testid="signup-username"><input data-testid="signup-password">
          <button data-testid="signup-submit">Sign up</button>
          <div data-testid="current-user" hidden></div>
          <button data-testid="slow-action">Start</button>
        \`;
        document.querySelector('[data-testid="signup-submit"]').onclick = () => {
          const current = document.querySelector('[data-testid="current-user"]');
          current.textContent = document.querySelector('[data-testid="signup-username"]').value;
          current.hidden = false;
        };
        document.querySelector('[data-testid="slow-action"]').onclick = () => {
          setTimeout(() => {
            const result = document.createElement('div');
            result.dataset.testid = 'slow-result';
            document.body.append(result);
          }, 6000);
        };
      }, 6000);
    </script>
  </body></html>`);
  try {
    const port = await server.port;
    await run(GRADER, ['--url', `http://127.0.0.1:${port}`, '--level', '1',
      '--spec', spec, '--out', out]);
    const report = readGradeArtifactPayload(out);
    const feature = first(report.features);
    assert.equal(feature.setupEvidence.status, 'passed');
    assert.equal(report.total, 1);
    assert.equal(report.max, 2);
    assert.equal(first(feature.criteria).evidence.status, 'passed');
    const failedCriterion = feature.criteria[1];
    assert(failedCriterion);
    assert.equal(failedCriterion.evidence.status, 'failed');
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
    const report = readGradeArtifactPayload(out);
    assert.equal(report.total, 0);
    assert.equal(report.max, 3);
    const feature = first(report.features);
    assert.equal(feature.max, 3);
    assert.equal(first(feature.criteria).evidence.status, 'inconclusive');
    assert.equal(first(report.inconclusive).points, 3);
  } finally {
    server.child.kill('SIGTERM');
    rmSync(root, { recursive: true, force: true });
  }
});
