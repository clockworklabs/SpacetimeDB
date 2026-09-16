import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test, { type TestContext } from 'node:test';
import { writeArtifact } from '../src/evidence/artifacts.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { inspectSavedDiagnostic } from '../src/runtime/saved-diagnostic.js';

const digest = (path: string) => createHash('sha256').update(readFileSync(path)).digest('hex');
function fixture(t: TestContext) {
  const root = mkdtempSync(join(tmpdir(), 'saved-diagnostic-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const app = join(root, 'source'), runPath = join(root, 'run.json'), checkpointPath = join(root, 'checkpoint.json');
  mkdirSync(app); writeFileSync(join(app, 'server.js'), 'accepted source');
  const sourceSha256 = hashAppSource(app).sha256, selectionSha256 = 'a'.repeat(64);
  const checkpoint = { backend: 'postgres', track: 'ecommerce', level: 3, source: { sha256: sourceSha256 },
    selection: { sha256: selectionSha256, checks: [{ stableKey: 'ecommerce.checkout.complete' }] } };
  const declaration = { sequence: 1, phase: 'final', level: 3, accepted: true, excluded: false, workNodeIds: [],
    sourceSha256, selectionSha256, evidence: { path: 'checkpoint.json', sha256: '0'.repeat(64) },
    cost: { status: 'exact', costUsd: 1 }, executionCost: { status: 'exact', costUsd: 1 },
    completion: { selected: 1, passed: 1, failed: 0, blocked: 0, unmeasured: 0, rate: 1 },
    checks: [{ id: 'ecommerce.checkout.complete', status: 'passed' }] };
  const run = { backend: 'postgres', track: 'ecommerce', contaminated: false,
    runtime: { buildImage: `sha256:${'b'.repeat(64)}` }, backendLease: { runIndex: 1 },
    condition: { guidance: { credentialAliases: { 'role:admin': 'admin' } } }, checkpoints: [declaration] };
  const readerPath = join(root, 'reader.json');
  writeFileSync(readerPath, JSON.stringify({ sourceSha256, sql: 'SELECT 1' }));
  const input = { run: 'run.json', runSha256: '', checkpoint: 1, source: 'source',
    reader: { path: 'reader.json', sha256: digest(readerPath) } };
  const save = (completedAt: string | null = '2026-09-15T12:01:00.000Z') => {
    writeArtifact(checkpointPath, { kind: 'grade_bundle', id: 'accepted-grade', payload: checkpoint });
    declaration.evidence.sha256 = digest(checkpointPath);
    writeArtifact(runPath, { kind: 'benchmark_run', id: 'saved-run', payload: run,
      timestamps: { startedAt: '2026-09-15T12:00:00.000Z', completedAt } });
    input.runSha256 = digest(runPath);
  };
  save();
  return { root, app, runPath, checkpointPath, readerPath, sourceSha256, checkpoint, declaration, run, input, save };
}

test('saved diagnostic binds accepted final source, original build and run index through real artifact envelopes', t => {
  const f = fixture(t), result = inspectSavedDiagnostic(f.input, f.root);
  assert.equal(result.sourceSha256, f.sourceSha256);
  assert.equal(result.source, f.app); assert.equal(result.run, f.runPath);
  assert.equal(result.reader.path, f.readerPath); assert.equal(result.reader.sha256, digest(f.readerPath));
  assert.equal(result.buildImage, f.run.runtime.buildImage); assert.equal(result.runIndex, 1);
  assert.equal(result.selectionSha256, f.declaration.selectionSha256);
  assert.deepEqual(result.credentialAliases, { 'role:admin': 'admin' });
});

test('saved diagnostic rejects changed run, checkpoint, source and reader bytes', t => {
  for (const target of ['run', 'checkpoint', 'source', 'reader'] as const) {
    const f = fixture(t);
    const path = { run: f.runPath, checkpoint: f.checkpointPath,
      source: join(f.app, 'server.js'), reader: f.readerPath }[target];
    appendFileSync(path, ' ');
    assert.throws(() => inspectSavedDiagnostic(f.input, f.root), /changed|mismatch|does not match/);
  }
  const f = fixture(t);
  writeFileSync(f.readerPath, JSON.stringify({ sourceSha256: 'c'.repeat(64), sql: 'SELECT 1' }));
  f.input.reader.sha256 = digest(f.readerPath);
  assert.throws(() => inspectSavedDiagnostic(f.input, f.root), /reader does not match/);
});

test('saved diagnostic requires a completed uncontaminated run and an unambiguous final accepted L3 checkpoint', t => {
  for (const mode of ['running', 'contaminated', 'rejected', 'excluded', 'earlier', 'duplicate', 'wrong-level']) {
    const f = fixture(t);
    if (mode === 'contaminated') f.run.contaminated = true;
    if (mode === 'rejected') f.declaration.accepted = false;
    if (mode === 'excluded') f.declaration.excluded = true;
    if (mode === 'earlier') f.run.checkpoints.push({ ...f.declaration, sequence: 2 });
    if (mode === 'duplicate') f.run.checkpoints.push({ ...f.declaration });
    if (mode === 'wrong-level') f.declaration.level = 2;
    f.save(mode === 'running' ? null : undefined);
    assert.throws(() => inspectSavedDiagnostic(f.input, f.root), /./, mode);
  }
});

test('saved diagnostic rejects source or selection substitution and payment/reservation selections', t => {
  for (const mode of ['source', 'selection', 'duplicate', 'payment-records', 'payment-deduplication', 'reservation']) {
    const f = fixture(t);
    if (mode === 'source') f.checkpoint.source.sha256 = 'c'.repeat(64);
    else if (mode === 'selection') f.checkpoint.selection.sha256 = 'c'.repeat(64);
    else if (mode === 'duplicate') f.checkpoint.selection.checks.push({ ...f.checkpoint.selection.checks[0]! });
    else f.checkpoint.selection.checks = [{ stableKey: `ecommerce.spec.${mode}.check` }];
    f.save();
    assert.throws(() => inspectSavedDiagnostic(f.input, f.root), /mismatch|order-only/, mode);
  }
});
