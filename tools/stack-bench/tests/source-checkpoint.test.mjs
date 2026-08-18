import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createArtifact, readArtifact } from '../src/evidence/artifacts.mjs';
import { preserveLevelCheckpoint } from '../src/runtime/source-checkpoint.mjs';

test('a level checkpoint preserves only source and binds it to the parent run', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-level-checkpoint-'));
  const app = join(root, 'app');
  const output = join(root, 'result');
  try {
    mkdirSync(join(app, 'src'), { recursive: true });
    writeFileSync(join(app, 'src', 'app.ts'), 'export const answer = 42;\n');
    mkdirSync(join(app, 'node_modules', 'dep'), { recursive: true });
    writeFileSync(join(app, 'node_modules', 'dep', 'index.js'), 'not source\n');
    writeFileSync(join(app, 'BUG_REPORT.md'), 'private repair evidence\n');

    const checkpoint = preserveLevelCheckpoint({
      appDir: app,
      outputDir: output,
      runId: 'run-parent',
      track: 'ecommerce',
      backend: 'postgres',
      level: 1,
      repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 3,
        stopReason: 'budget-exhausted' },
      outcome: { kind: 'app_failure', failed: 1 },
      selectionSha256: 'a'.repeat(64),
    });

    const artifact = readArtifact(join(output, checkpoint.artifact),
      { expectedKind: 'source_checkpoint', expectedId: 'run-parent-l1-checkpoint' });
    assert.equal(artifact.attempt.parentId, 'run-parent');
    assert.equal(artifact.payload.source.sha256, checkpoint.sha256);
    assert.equal(artifact.payload.source.directory, 'level-l1-source');
    assert.equal(existsSync(join(output, checkpoint.directory, 'src', 'app.ts')), true);
    assert.equal(existsSync(join(output, checkpoint.directory, 'node_modules')), false);
    assert.equal(existsSync(join(output, checkpoint.directory, 'BUG_REPORT.md')), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('source checkpoint artifacts reject paths and repair accounting that cannot be trusted', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-level-checkpoint-invalid-'));
  const app = join(root, 'app');
  try {
    mkdirSync(app, { recursive: true });
    writeFileSync(join(app, 'app.js'), 'export default true;\n');
    assert.throws(() => createArtifact({
      kind: 'source_checkpoint',
      id: 'wrong-path',
      payload: {
        schemaVersion: 1,
        track: 'ecommerce',
        backend: 'postgres',
        level: 1,
        source: { directory: '../outside', sha256: 'a'.repeat(64), files: 1 },
        repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 3,
          stopReason: 'budget-exhausted' },
        outcome: { kind: 'app_failure' },
        selectionSha256: null,
      },
    }), /directory does not match its level/);
    assert.throws(() => preserveLevelCheckpoint({
      appDir: app,
      outputDir: join(root, 'result'),
      runId: 'run-parent',
      track: 'ecommerce',
      backend: 'postgres',
      level: 1,
      repair: { status: 'budget-exhausted', budgetRounds: 3, roundsUsed: 4,
        stopReason: 'budget-exhausted' },
      outcome: { kind: 'app_failure' },
    }), /roundsUsed exceeds its budget/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
