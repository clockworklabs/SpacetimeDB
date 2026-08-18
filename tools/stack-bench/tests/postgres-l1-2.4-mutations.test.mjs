import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { mutationFileEdits, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture } from '../src/references/reference-fixtures.mjs';

const ROOT = join(import.meta.dirname, '..');
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const RECIPE = 'ecommerce.l1-modular@2.4.0';
const FIXTURE_SHA256 = '389d778f1835377fd2f92864d6afa20851c65076c80ddecba1e860cf7f4d9ec9';
const MANIFEST = join(ROOT, 'grader', 'mutations', 'candidates',
  'postgres-ecom-l1-modular-2.4.0.json');
const EXPECTED_MISSING = [];

const manifest = JSON.parse(readFileSync(MANIFEST, 'utf8'));
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'l1-modular-2.4.0.json'), { trackRoot: TRACK });
const stableByTarget = new Map(release.checkCatalog.map(check => [
  `${check.source}:${check.featureId}:${check.criterionId}`,
  check.stableKey,
]));

test('PostgreSQL L1 2.4 candidate mutations bind to the exact prepared fixture', () => {
  assert.deepEqual({ schemaVersion: manifest.schemaVersion, status: manifest.status,
    backend: manifest.backend, track: manifest.track, level: manifest.level }, {
    schemaVersion: 1, status: 'candidate', backend: 'postgres', track: 'ecommerce', level: 1,
  });
  assert.equal(manifest.fixtureSha256, FIXTURE_SHA256);
  assert.equal(Object.hasOwn(manifest, 'scenario'), false);
  assert.equal(new Set(manifest.mutations.map(mutation => mutation.id)).size,
    manifest.mutations.length);
  assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
    requireScenario: true,
  }).issues, []);

  const fixture = selectReferenceFixture(loadReferenceRegistry(), {
    backend: 'postgres', track: 'ecommerce', level: 1, recipe: RECIPE,
  });
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-postgres-l1-2.4-mutations-'));
  try {
    const prepared = prepareReferenceFixtureSource(fixture, app);
    assert.equal(prepared.sha256, FIXTURE_SHA256);
  } finally {
    rmSync(app, { recursive: true, force: true });
  }
});

test('every PostgreSQL target resolves to the exact L1 2.4 release scenario', t => {
  const covered = new Set();
  for (const mutation of manifest.mutations) {
    const scenario = mutation.scenario.replaceAll('\\', '/')
      .replace(/^tracks\/ecommerce\//, '');
    for (const target of mutationTargetKeys(mutation)) {
      const separator = target.indexOf(':');
      const stableKey = stableByTarget.get(
        `${scenario}:${target.slice(0, separator)}:${target.slice(separator + 1)}`);
      assert(stableKey, `${mutation.id} target ${scenario}:${target} is not in ${RECIPE}`);
      covered.add(stableKey);
    }
  }

  const scored = release.checkCatalog.filter(check => check.points > 0)
    .map(check => check.stableKey).sort();
  assert.equal(scored.length, 46);
  const missing = scored.filter(stableKey => !covered.has(stableKey));
  assert.deepEqual(missing, EXPECTED_MISSING);
  assert.equal(covered.size, 46);
  t.diagnostic(`46/46 scored keys covered; missing: ${missing.join(', ') || 'none'}`);
});

test('every PostgreSQL mutation anchor applies exactly once and remains valid TypeScript', () => {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), {
    backend: 'postgres', track: 'ecommerce', level: 1, recipe: RECIPE,
  });
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-postgres-l1-2.4-sources-'));
  try {
    prepareReferenceFixtureSource(fixture, app);
    for (const mutation of manifest.mutations) {
      const sources = new Map();
      for (const edit of mutationFileEdits(mutation)) {
        const path = join(app, ...edit.file.split('/'));
        let mutated = sources.get(edit.file) ?? readFileSync(path, 'utf8');
        assert.equal(mutated.split(edit.find).length - 1, 1,
          `${mutation.id} anchor must match the exact prepared source once`);
        mutated = mutated.replace(edit.find, edit.replace);
        sources.set(edit.file, mutated);
      }
      for (const [file, mutated] of sources) {
        const transpiled = ts.transpileModule(mutated, {
          compilerOptions: {
            jsx: ts.JsxEmit.ReactJSX,
            module: ts.ModuleKind.ESNext,
            target: ts.ScriptTarget.ES2022,
          },
          fileName: file,
          reportDiagnostics: true,
        });
        assert.deepEqual((transpiled.diagnostics ?? [])
          .filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error)
          .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')), [],
        `${mutation.id}:${file} must remain syntactically valid`);
      }
    }
  } finally {
    rmSync(app, { recursive: true, force: true });
  }
});
