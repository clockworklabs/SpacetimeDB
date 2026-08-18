import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { mutationFileEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { loadReferenceRegistry, prepareReferenceFixtureSource } from '../src/references/reference-fixtures.mjs';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const MUTATIONS = join(ROOT, 'grader', 'mutations');

test('every mutation manifest binds valid edits to exact scenario criteria', () => {
  const files = readdirSync(MUTATIONS).filter(file => file.endsWith('.json')).sort();
  assert.ok(files.length > 0, 'expected mutation manifests');
  for (const file of files) {
    const manifest = JSON.parse(readFileSync(join(MUTATIONS, file), 'utf8'));
    const definitions = validateMutationDefinitions(manifest.mutations,
      { defaultScenario: manifest.scenario, requireScenario: true });
    assert.deepEqual(definitions.issues, [], `${file} has an invalid mutation definition`);
    assert.match(manifest.status, /^(active|candidate|legacy-unreproducible)$/);
    if (manifest.status !== 'legacy-unreproducible') assert.equal(manifest.schemaVersion, 1);
    assert.match(manifest.backend, /^(spacetime|postgres|mongodb)$/);
    assert.equal(typeof manifest.track, 'string');
    assert.ok(Number.isInteger(manifest.level) && manifest.level > 0);
    if (manifest.status !== 'legacy-unreproducible') {
      assert.match(manifest.fixtureSha256, /^[a-f0-9]{64}$/);
    }

    for (const mutation of manifest.mutations) {
      const scenarioPath = mutationScenario(manifest, mutation);
      assert.equal(typeof scenarioPath, 'string');
      const scenario = JSON.parse(readFileSync(join(ROOT, scenarioPath), 'utf8'));
      for (const target of mutationTargetKeys(mutation)) {
        const [featureId, ...criterionParts] = target.split(':');
        const feature = scenario.features.find(candidate => String(candidate.id) === featureId);
        assert.ok(feature, `${file}/${mutation.id} names missing feature ${featureId}`);
        const criteria = new Set(feature.criteria.map(criterion => String(criterion.id)));
        const criterion = criterionParts.join(':');
        assert.ok(criteria.has(criterion), `${file}/${mutation.id} names missing criterion ${target}`);
      }
    }
  }
});

test('current mutation anchors match their hash-bound canonical fixture exactly once', () => {
  const registry = loadReferenceRegistry();
  for (const fixture of registry.fixtures.filter(candidate =>
    candidate.status === 'candidate' || candidate.status === 'active')) {
    if (!fixture.mutationManifests.length) continue;
    const temporary = fixture.source
      ? mkdtempSync(join(tmpdir(), `stack-bench-mutation-fixture-${fixture.backend}-`)) : null;
    try {
      const sourceRoot = temporary ?? join(ROOT, fixture.targetPath);
      if (temporary) prepareReferenceFixtureSource(fixture, temporary);
      for (const relativeManifest of fixture.mutationManifests) {
        const manifest = JSON.parse(readFileSync(join(ROOT, relativeManifest), 'utf8'));
        assert.equal(manifest.fixtureSha256, fixture.imported.sourceSha256,
          `${fixture.id} manifest is bound to different source bytes`);
        for (const mutation of manifest.mutations) {
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
