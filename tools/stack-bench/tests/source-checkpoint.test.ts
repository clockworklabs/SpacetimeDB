import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { createArtifact, readArtifact, writeArtifact, writeRunJson } from '../src/evidence/artifacts.js';
import { preserveLevelCheckpoint } from '../src/runtime/source-checkpoint.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { inspectGradeSource } from '../commands/bench.js';

interface CheckpointPayload extends Record<string, unknown> {
  source: { sha256: string; directory: string };
}

test('dependency diagnostics bind the rejected first-build source and its own check scope', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-dependency-replay-'));
  const output = join(root, 'execution');
  const sourcePath = join(output, 'first-build-l2');
  const bundlePath = join(output, 'first-build-l2-grading', 'bundle.json');
  try {
    mkdirSync(sourcePath, { recursive: true });
    mkdirSync(join(output, 'source'));
    writeFileSync(join(sourcePath, 'app.js'), 'export const depth = 2;\n');
    writeFileSync(join(output, 'source', 'app.js'), 'export const depth = 1;\n');
    const source = hashAppSource(sourcePath);
    const check = 'ecommerce.spec.access-control.warehouse-write-boundary.103b';
    const selection = { schemaVersion: 3, sha256: 'a'.repeat(64),
      recipe: { id: 'ecommerce.progression-catalog', sha256: 'd'.repeat(64) },
      requested: { features: ['ecommerce.feature.warehouse-admin'], checks: [check],
        specifications: { requested: [], expected: ['ecommerce.spec.access-control'], observed: [] },
        dependencyExpansion: 'exact' },
      scoredChecks: [{ stableKey: check, points: 2 }], observedChecks: [] };
    const run = {
      id: 'dependency-parent', startedAt: '2026-09-06T12:00:00.000Z',
      completedAt: '2026-09-06T12:01:00.000Z', contaminated: false,
      track: 'ecommerce', backend: 'postgres', model: 'test', mode: { id: 'dependency' },
      identities: { agentAdapter: { id: 'claude-code' } },
      backendLease: { runIndex: 2, resources: { serverUri: null as string | null } },
      runtime: { buildImage: 'test-image' },
      condition: { guidance: { credentialAliases: { staff: 'saved-staff' } }, requested: {
        levels: [{ level: 2, recipe: { id: selection.recipe.id, contentSha256: selection.recipe.sha256 },
          selection: { ...selection, sha256: 'e'.repeat(64), scoredChecks: [
            ...selection.scoredChecks, { stableKey: 'never-attempted', points: 1 }] },
          task: { mode: 'upgrade', contractSha256: 'b'.repeat(64), requirementSha256: 'c'.repeat(64) } }],
      } },
      levels: [{ level: 2, selection, checkpoint: null, repairs: 0,
        firstBuild: { source: { sha256: source.sha256, files: source.files.length },
          outcome: { kind: 'inconclusive' } }, outcome: { kind: 'inconclusive' } }],
    };
    const save = () => writeRunJson(join(output, 'run.json'), run);
    save();
    writeArtifact(bundlePath, { kind: 'grade_bundle', id: 'dependency-parent-grade-l2',
      attempt: { id: 'dependency-parent-grade-l2', parentId: run.id },
      identities: { recipe: selection.recipe },
      payload: { backend: 'postgres', track: 'ecommerce', level: 2, observation: 'scored',
        source: { sha256: source.sha256 }, selection,
        recipeRelease: { id: selection.recipe.id, contentSha256: selection.recipe.sha256 },
        outcome: { kind: 'inconclusive' } } });
    const before = readFileSync(join(output, 'run.json'));
    const replay = inspectGradeSource(output, { level: 2, checkKeys: [check] });
    assert.equal(replay.sourcePath, sourcePath);
    assert.equal(replay.source.sha256, source.sha256);
    assert.equal(replay.serverUri, null);
    assert.deepEqual(replay.aliases, { staff: 'saved-staff' });
    assert.deepEqual(readFileSync(join(output, 'run.json')), before);
    assert.throws(() => inspectGradeSource(output), /grade-level/);
    assert.throws(() => inspectGradeSource(output, { level: 2 }), /check/);
    assert.throws(() => inspectGradeSource(output, { level: 3, checkKeys: [check] }), /level|depth|candidate/);
    assert.throws(() => inspectGradeSource(output, { level: 2, checkKeys: ['never-attempted'] }), /scope|check/);
    run.levels.push(run.levels[0]!);
    save();
    assert.throws(() => inspectGradeSource(output, { level: 2, checkKeys: [check] }), /unique|ambiguous|exactly one/);
    run.levels.pop();
    run.contaminated = true;
    save();
    assert.throws(() => inspectGradeSource(output, { level: 2, checkKeys: [check] }), /contaminat/);
    run.contaminated = false;
    save();
    const grade = JSON.parse(readFileSync(bundlePath, 'utf8'));
    grade.attempt.parentId = 'other-run';
    writeFileSync(bundlePath, JSON.stringify(grade));
    assert.throws(() => inspectGradeSource(output, { level: 2, checkKeys: [check] }), /parent|match/);
    grade.attempt.parentId = run.id;
    run.backend = 'spacetime';
    run.backendLease.runIndex = 7;
    run.backendLease.resources.serverUri = 'http://127.0.0.1:3217';
    grade.payload.backend = 'spacetime';
    writeFileSync(bundlePath, JSON.stringify(grade));
    save();
    const spacetimeReplay = inspectGradeSource(output, { level: 2, checkKeys: [check] });
    assert.equal(spacetimeReplay.serverUri, 'http://127.0.0.1:3217');
    assert.equal(spacetimeReplay.parent.payload.backendLease.runIndex, 7);
    for (const uri of [null, 'http://remote.example:3217', 'https://127.0.0.1:3217', 'http://127.0.0.1']) {
      run.backendLease.resources.serverUri = uri;
      save();
      const rejected = spawnSync(process.execPath,
        [fileURLToPath(new URL('../commands/bench.js', import.meta.url)), '--grade-from', output,
          '--grade-level', '2', '--out', join(root, 'regrade'), '--check', check],
        { encoding: 'utf8' });
      assert.equal(rejected.status, 1);
      assert.match(rejected.stderr, /serverUri/);
      assert.doesNotMatch(rejected.stderr, /preflight|Docker|docker/);
    }
    run.backendLease.resources.serverUri = 'http://127.0.0.1:3217';
    save();
    writeFileSync(join(sourcePath, 'app.js'), 'changed candidate');
    assert.throws(() => inspectGradeSource(output, { level: 2, checkKeys: [check] }), /source|match/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

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
      repair: { status: 'budget-exhausted', limit: 3, used: 3,
        stopReason: 'budget-exhausted' },
      outcome: { kind: 'app_failure', failed: 1 },
      selectionSha256: 'a'.repeat(64),
    });

    const artifact = readArtifact<CheckpointPayload>(join(output, checkpoint.artifact),
      { expectedKind: 'source_checkpoint', expectedId: 'run-parent-l1-checkpoint' });
    assert.equal(artifact.attempt.parentId, 'run-parent');
    assert.equal(artifact.payload.source.sha256, checkpoint.sha256);
    assert.equal(artifact.payload.source.directory, 'level-l1-source');
    assert.equal(existsSync(join(output, checkpoint.directory, 'src', 'app.ts')), true);
    assert.equal(existsSync(join(output, checkpoint.directory, 'node_modules')), false);
    assert.equal(existsSync(join(output, checkpoint.directory, 'BUG_REPORT.md')), false);

    const selection = { schemaVersion: 3, sha256: 'a'.repeat(64),
      requested: { features: [], checks: [] }, scoredChecks: [] };
    writeRunJson(join(output, 'run.json'), {
      id: 'run-parent', startedAt: '2026-09-05T12:00:00.000Z',
      completedAt: '2026-09-05T12:01:00.000Z',
      track: 'ecommerce', backend: 'postgres', model: 'test', mode: { id: 'sequential' },
      identities: { agentAdapter: { id: 'claude-code' } },
      backendLease: { runIndex: 0 }, runtime: { buildImage: 'test-image' },
      condition: { guidance: { credentialAliases: { customer: 'saved-customer' } },
        requested: { levels: [{ level: 1, selection,
          task: { contractSha256: 'b'.repeat(64), requirementSha256: 'c'.repeat(64) } }] } },
      levels: [{ level: 1, checkpoint }],
    });
    const regrade = inspectGradeSource(output);
    assert.equal(regrade.source.sha256, checkpoint.sha256);
    assert.deepEqual(regrade.aliases, { customer: 'saved-customer' });
    const outsideScope = spawnSync(process.execPath,
      [fileURLToPath(new URL('../commands/bench.js', import.meta.url)), '--grade-from', output,
        '--out', join(root, 'regrade'), '--check', 'not-in-the-original-scope'], { encoding: 'utf8' });
    assert.equal(outsideScope.status, 1);
    assert.match(outsideScope.stderr, /regrade checks must belong to the original scored scope/);
    writeFileSync(join(output, checkpoint.directory, 'src', 'app.ts'), 'changed source');
    assert.throws(() => inspectGradeSource(output), /does not match its parent/);
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
        schemaVersion: 3,
        track: 'ecommerce',
        backend: 'postgres',
        level: 1,
        source: { directory: '../outside', sha256: 'a'.repeat(64), files: 1 },
        repair: { status: 'budget-exhausted', limit: 3, used: 3,
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
      repair: { status: 'budget-exhausted', limit: 3, used: 4,
        stopReason: 'budget-exhausted' },
      outcome: { kind: 'app_failure' },
    }), /used exceeds its budget/);
    assert.throws(() => preserveLevelCheckpoint({
      appDir: app,
      outputDir: join(root, 'feature-repairs'),
      runId: 'run-parent',
      track: 'ecommerce',
      backend: 'postgres',
      level: 1,
      repair: { status: 'budget-exhausted', limit: 2, used: 2,
        stopReason: 'budget-exhausted', nodeRepairs: [
          { nodeId: 'accounts', used: -1, exhaustionReason: null },
        ] },
      outcome: { kind: 'app_failure' },
    }), /nodeRepairs\[0\] is invalid/);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
