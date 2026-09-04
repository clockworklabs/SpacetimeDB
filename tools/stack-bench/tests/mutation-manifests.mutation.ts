import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { mutationFileEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions, type MutationManifest }
  from '../src/evidence/mutation-analysis.js';
import { loadReferenceRegistry, prepareReferenceFixtureSource } from '../src/references/reference-fixtures.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const MUTATIONS = join(STACK_BENCH_ROOT, 'grader', 'mutations');

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function isMutationManifest(value: unknown): value is MutationManifest & Record<string, unknown> {
  return record(value) && (value.mutations === undefined
    || Array.isArray(value.mutations) && value.mutations.every(record));
}

function manifestAt(path: string): MutationManifest & Record<string, unknown> {
  const value = readJson(path);
  if (!isMutationManifest(value)) throw new Error(`${path} mutations must be an array`);
  return value;
}

test('every mutation manifest binds valid edits to exact scenario criteria', () => {
  const files = readdirSync(MUTATIONS).filter(file => file.endsWith('.json')).sort();
  assert.ok(files.length > 0, 'expected mutation manifests');
  for (const file of files) {
    const manifest = manifestAt(join(MUTATIONS, file));
    const definitions = validateMutationDefinitions(manifest.mutations,
      { defaultScenario: typeof manifest.scenario === 'string' ? manifest.scenario : null, requireScenario: true });
    assert.deepEqual(definitions.issues, [], `${file} has an invalid mutation definition`);
    assert.equal(manifest.schemaVersion, 3);
    assert.deepEqual(Object.keys(manifest).filter(field => !new Set([
      'schemaVersion', 'fixtureSha256', 'backend', 'track', 'scenario', 'note', 'mutations',
    ]).has(field)), [], `${file} has unknown root fields`);
    assert(typeof manifest.backend === 'string');
    assert.match(manifest.backend, /^(spacetime|postgres|mongodb)$/);
    assert.equal(typeof manifest.track, 'string');
    assert.equal(manifest.level, undefined);
    assert(typeof manifest.fixtureSha256 === 'string');
    assert.match(manifest.fixtureSha256, /^[a-f0-9]{64}$/);

    for (const mutation of manifest.mutations ?? []) {
      const scenarioPath = mutationScenario(manifest, mutation);
      assert.equal(typeof scenarioPath, 'string');
      for (const target of mutationTargetKeys(mutation)) {
        assert.match(target, /^[a-z0-9][a-z0-9.-]+$/,
          `${file}/${mutation.id} has an invalid stable check ID`);
      }
    }
  }
});

test('current mutation anchors match their hash-bound canonical fixture exactly once', () => {
  const registry = loadReferenceRegistry();
  for (const fixture of registry.fixtures) {
    const manifests = fixture.mutationManifests ?? [];
    if (!manifests.length) continue;
    if ((!fixture.source && !fixture.targetPath) || !fixture.imported?.sourceSha256) {
      throw new Error(`${fixture.id} mutation fixture has incomplete source identity`);
    }
    const temporary = fixture.source
      ? mkdtempSync(join(tmpdir(), `stack-bench-mutation-fixture-${fixture.backend}-`)) : null;
    try {
      let sourceRoot: string;
      if (temporary) {
        prepareReferenceFixtureSource(fixture, temporary);
        sourceRoot = temporary;
      } else {
        if (!fixture.targetPath) throw new Error(`${fixture.id} has no target path`);
        sourceRoot = join(STACK_BENCH_ROOT, fixture.targetPath);
      }
      for (const relativeManifest of manifests) {
        const manifest = manifestAt(join(STACK_BENCH_ROOT, relativeManifest));
        assert(typeof manifest.fixtureSha256 === 'string');
        assert.equal(manifest.fixtureSha256, fixture.imported.sourceSha256,
          `${fixture.id} manifest is bound to different source bytes`);
        for (const mutation of manifest.mutations ?? []) {
          for (const edit of mutationFileEdits(mutation)) {
            const source = readFileSync(join(sourceRoot, edit.file), 'utf8');
            const matches = source.split(edit.find).length - 1;
            assert.equal(matches, 1,
              `${fixture.id}/${mutation.id} anchor matched ${matches} times in ${edit.file}`);
          }
        }
      }
    } finally {
      if (temporary) rmSync(temporary, { recursive: true, force: true });
    }
  }
});
