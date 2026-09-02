import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { mutationScenario, mutationTargetKeys,
  readMutationManifest, validateMutationDefinitions, type LoadedMutationDefinition }
  from '../src/evidence/mutation-analysis.js';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture, type ReferenceFixtureSelector } from '../src/references/reference-fixtures.js';
import { buildRecipeRelease, type RecipeRelease } from '../src/composition/recipe-release.js';
import { selectScenarioChecks } from '../src/composition/recipe-selection.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

const ROOT = STACK_BENCH_ROOT;
const TRACK = join(ROOT, 'tracks', 'ecommerce');
const release = buildRecipeRelease(join(TRACK, 'composition', 'recipes',
  'sequential-l1-2.5.0.json'));

function prepareReferenceSource(args: ReferenceFixtureSelector & { app: string }) {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), args);
  const prepared = prepareReferenceFixtureSource(fixture, args.app);
  return { fixture, sourceSha256: prepared.sha256 };
}

interface CandidateCase {
  backend: string;
  sourceSha256: string;
  manifest: string;
  lastUnitMutation: string;
}
const cases: CandidateCase[] = [
  {
    backend: 'mongodb',
    sourceSha256: 'feeaf484f5c5d2eae6f61b192e3e50b0a7e6da85e2f761685e33261708acf8c6',
    manifest: 'mongodb-ecommerce-2.0.1.json',
    lastUnitMutation: 'last-unit-allows-negative-stock',
  },
  {
    backend: 'postgres',
    sourceSha256: 'e461967ff8b99394c278e6d382e455a7fc3086c88016543fa668baf808baec0d',
    manifest: 'postgres-ecommerce-2.0.1.json',
    lastUnitMutation: 'oversell-no-row-lock',
  },
  {
    backend: 'spacetime',
    sourceSha256: '58a193d61e02eac8e2c4801dc4cefb78db42468515a452a17e2544f2132148ce',
    manifest: 'spacetime-ecommerce-2.0.1.json',
    lastUnitMutation: 'purchase-does-not-reserve-stock-last-unit',
  },
];

function byStableKey(candidate: RecipeRelease) {
  return new Map(candidate.checkCatalog.map(check => [check.stableKey, check]));
}

test('current L1 has the complete scored concurrency and live-state surface', () => {
  assert.equal(release.checkCatalog.length, 48);
  assert.equal(release.scoring.points, 58);
  assert.equal(release.checkCatalog.filter(check => check.points === 0).length, 2);
  const restockControl = byStableKey(release)
    .get('ecommerce.spec.concurrency-safety.restock-race.202-control');
  assert(restockControl, 'the restock control must exist');
  assert.equal(restockControl.points, 0,
    'the ordinary-restock precondition must not double-count already-scored restock behavior');
});

test('each last-unit score retains the shared purchase race when selected alone', () => {
  const scenarioPath = join(TRACK, 'scenarios', '01-last-unit-2.3.0.json');
  const scenario = compileScenarioDefinition(readJson(scenarioPath), { source: scenarioPath });
  for (const criterionId of ['201a', '201b', '201c']) {
    const key = `ecommerce.spec.concurrency-safety.last-unit.${criterionId}`;
    const selected = selectScenarioChecks(scenario, { checks: release.checkCatalog }, [key]);
    const [feature] = selected.features;
    assert(feature, 'the selected scenario must have a feature');
    assert.deepEqual(feature.criteria.map(criterion => criterion.id), [criterionId]);
    const races = feature.setup.filter(step => step.do === 'clickConcurrently');
    assert.equal(races.length, 1);
    const [race] = races;
    assert(race, 'the selected scenario must have a race');
    assert.deepEqual(race.actors, ['a', 'b', 'c', 'd', 'e', 'f']);
    assert.equal(race.testid, 'buy-now');
    assert.equal(recordValue(race.in, 'race locator').contains, 'Air Purifier');
    const selectedCriterion = feature.criteria.find(criterion => criterion.id === criterionId);
    assert(selectedCriterion, 'the selected feature must retain its criterion');
    assert.equal(selectedCriterion.steps.some(step => step.do === 'clickConcurrently'
      && step.testid === 'buy-now'), false);
    const baseline = feature.setup.findIndex(step => step.as === 'revenue-before-last-unit');
    assert(baseline >= 0 && baseline < feature.setup.indexOf(race),
      'the revenue baseline must be recorded before the shared purchase race');
  }
});

test('the restock race retains its admin page prerequisite when selected alone', () => {
  const scenarioPath = join(TRACK, 'scenarios', '01-restock-race-2.3.0.json');
  const scenario = compileScenarioDefinition(readJson(scenarioPath), { source: scenarioPath });
  const key = 'ecommerce.spec.concurrency-safety.restock-race.202a';
  const selected = selectScenarioChecks(scenario, { checks: release.checkCatalog }, [key]);
  const [feature] = selected.features;
  assert(feature, 'the selected scenario must have a feature');
  assert.deepEqual(feature.criteria.map(criterion => criterion.id), ['202a']);
  assert(feature.setup.some(step => step.do === 'click' && step.actor === 'admin'
    && step.testid === 'admin-link'));
  const [criterion] = feature.criteria;
  assert(criterion, 'the selected feature must have a criterion');
  assert(criterion.steps.some(step => step.do === 'race'));
});

test('duplicate checkout metadata describes the current cross-stack named action', () => {
  const scenarioPath = join(TRACK, 'scenarios', '01-duplicate-checkout-2.3.0.json');
  const scenario = compileScenarioDefinition(readJson(scenarioPath), { source: scenarioPath });
  const [feature] = scenario.features;
  assert(feature, 'duplicate checkout must have a feature');
  const checkout = feature.criteria.find(criterion => criterion.id === '203b');
  assert(checkout, 'duplicate checkout must have criterion 203b');
  const metadata = [checkout.note, checkout.provenBy, checkout.withheld].join('\n');
  assert.match(metadata, /callConcurrently/);
  assert.match(metadata, /MongoDB/);
  assert.match(metadata, /PostgreSQL/);
  assert.match(metadata, /SpacetimeDB/);
  assert.match(metadata, /Docker mutation qualification/);
  assert.doesNotMatch(metadata, /replayConcurrently|INCONCLUSIVE|lastWrites|captured HTTP write/);
  assert.deepEqual(checkout.steps.filter(step => ['callConcurrently', 'expectCallOutcomes']
    .includes(step.do)).map(step => step.do), ['callConcurrently', 'expectCallOutcomes']);
});

for (const entry of cases) {
test(`${entry.backend} binds the current L1 mutation inventory to its effective source`, () => {
    const work = mkdtempSync(join(tmpdir(), `stack-bench-l1-concurrency-${entry.backend}-`));
    try {
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend: entry.backend,
        track: 'ecommerce',
        level: 1,
        recipe: 'ecommerce.sequential-l1@2.5.0',
        app,
      });
      assert.equal(prepared.sourceSha256, entry.sourceSha256);

      const manifest = readMutationManifest(join(ROOT, 'grader', 'mutations', entry.manifest));
      assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        defaultScenario: manifest.scenario,
        requireScenario: true,
      }).issues, []);

      assert.equal(manifest.mutations.some(mutation => mutationTargetKeys(mutation)
        .includes('ecommerce.spec.external-data-sync.external-stock.901b')), false, '901b has no deterministic candidate mutation');
      const mutations = new Map(manifest.mutations.map(mutation => [mutation.id, mutation]));
      const lastUnit = requiredMutation(mutations, entry.lastUnitMutation);
      assert.equal(mutationScenario(manifest, lastUnit),
        'tracks/ecommerce/scenarios/01-last-unit-2.3.1.json');
      assert.deepEqual(mutationTargetKeys(lastUnit), ['ecommerce.spec.concurrency-safety.last-unit.201a', 'ecommerce.spec.concurrency-safety.last-unit.201b', 'ecommerce.spec.concurrency-safety.last-unit.201c']);

      const purchase = requiredMutation(mutations, 'purchase-does-not-reserve-stock-restock-race');
      assert.equal(mutationScenario(manifest, purchase),
        'tracks/ecommerce/scenarios/01-restock-race-2.3.0.json');
      assert.deepEqual(mutationTargetKeys(purchase), ['ecommerce.spec.concurrency-safety.restock-race.202a']);
      const releaseKeys = new Set(release.checkCatalog.map(check => check.stableKey));
      for (const mutation of manifest.mutations.filter(candidate =>
        mutationTargetKeys(candidate).some(key => releaseKeys.has(key)))) {
        for (const key of mutationTargetKeys(mutation)) {
          assert(releaseKeys.has(key),
          `${mutation.id} must target an exact check in the current release`);
        }
        assert(mutation.file, `${mutation.id} must declare a source file`);
        const source = readFileSync(join(app, mutation.file), 'utf8');
        for (const edit of mutation.edits) {
          assert.equal(source.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match exactly once`);
          const mutated = source.replace(edit.find, edit.replace);
          const transpiled = ts.transpileModule(mutated, {
            compilerOptions: { module: ts.ModuleKind.ESNext, target: ts.ScriptTarget.ES2022 },
            fileName: mutation.file,
            reportDiagnostics: true,
          });
          assert.deepEqual((transpiled.diagnostics ?? [])
            .filter(diagnostic => diagnostic.category === ts.DiagnosticCategory.Error)
            .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')), []);
        }
      }
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
}

function readJson(path: string): unknown { return JSON.parse(readFileSync(path, 'utf8')); }
function recordValue(value: unknown, label: string): Record<string, unknown> {
  if (!isRecord(value)) throw new Error(`${label} must be an object`);
  return value;
}
function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}
function requiredMutation(mutations: Map<string, LoadedMutationDefinition>, id: string): LoadedMutationDefinition {
  const mutation = mutations.get(id);
  if (!mutation) throw new Error(`mutation ${id} is required`);
  return mutation;
}
