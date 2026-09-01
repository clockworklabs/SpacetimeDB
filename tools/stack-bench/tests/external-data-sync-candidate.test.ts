import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition, type CompiledCriterion, type CompiledFeature }
  from '../src/composition/definition-compiler.js';
import { mutationFileEdits, mutationScenario, readMutationManifest,
  validateMutationDefinitions, type LoadedMutationDefinition,
  type LoadedMutationManifest, type MutationFileEdit }
  from '../src/evidence/mutation-analysis.js';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture, type ReferenceFixtureSelector }
  from '../src/references/reference-fixtures.js';
import { buildRecipeRelease } from '../src/composition/recipe-release.js';
import { selectScenarioChecks } from '../src/composition/recipe-selection.js';

const ROOT = STACK_BENCH_ROOT;
const LIVE_SCENARIO = 'tracks/ecommerce/scenarios/01-external-live-sync-1.1.0.json';
const RELOAD_SCENARIO = 'tracks/ecommerce/scenarios/01-external-reload-sync-1.1.0.json';
const RESTART_SCENARIO = 'tracks/ecommerce/scenarios/01-external-server-restart-sync-1.1.0.json';
const RECONNECT_SCENARIO = 'tracks/ecommerce/scenarios/01-external-reconnect-sync-1.1.0.json';
const PACK = 'tracks/ecommerce/composition/packs/spec-external-data-sync-1.1.0.json';
const registry = loadReferenceRegistry();

function prepareReferenceSource(args: ReferenceFixtureSelector & { app: string }) {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), args);
  const prepared = prepareReferenceFixtureSource(fixture, args.app);
  return { fixture, sourceSha256: prepared.sha256 };
}

interface ExternalSyncCase {
  backend: string;
  fixtureSha256: string;
  mutations: Array<[id: string, scenario: string, targets: string[]]>;
}

const cases: ExternalSyncCase[] = [
  {
    backend: 'mongodb',
    fixtureSha256: 'b38c9ccd5bcdbb092f44b2f5f42674273c6f22d4b148be55fdf2c999df3475cf',
    mutations: [
      ['external-stock-polling-disabled', LIVE_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901a']],
      ['server-restart-disables-catalog-recovery', RESTART_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901c']],
      ['reconnect-generation-ignores-current-catalog', RECONNECT_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901d']],
    ],
  },
  {
    backend: 'postgres',
    fixtureSha256: 'ad3a7237947690a409039c1e32c9012333c0c03fdd60aeee240b10743a8320d5',
    mutations: [
      ['external-stock-polling-disabled', LIVE_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901a']],
      ['server-restart-does-not-resynchronize-catalog', RESTART_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901c']],
      ['reconnect-does-not-send-current-catalog', RECONNECT_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901d']],
    ],
  },
  {
    backend: 'spacetime',
    fixtureSha256: 'c09cfbbaf76ab99765b110db3d0ccf04ec6147b03c005ae49e074f094927fcb5',
    mutations: [
      ['stock-subscription-snapshotted-once', LIVE_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901a']],
      ['stock-view-ignores-update-across-app-server-stop', RESTART_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901c']],
      ['stock-view-keeps-pre-reconnect-snapshot', RECONNECT_SCENARIO, ['ecommerce.spec.external-data-sync.external-stock.901d']],
    ],
  },
];

function json(relative: string): unknown {
  return JSON.parse(readFileSync(join(ROOT, relative), 'utf8'));
}

test('external synchronization scenarios are focused and state-independent', () => {
  const live = compileScenarioDefinition(json(LIVE_SCENARIO), {
    source: LIVE_SCENARIO,
    expectedLevel: 1,
  });
  const reconnect = compileScenarioDefinition(json(RECONNECT_SCENARIO), {
    source: RECONNECT_SCENARIO,
    expectedLevel: 1,
  });
  const reload = compileScenarioDefinition(json(RELOAD_SCENARIO), {
    source: RELOAD_SCENARIO,
    expectedLevel: 1,
  });
  const restart = compileScenarioDefinition(json(RESTART_SCENARIO), {
    source: RESTART_SCENARIO,
    expectedLevel: 1,
  });

  const liveFeature = requiredFeature(live.features[0], LIVE_SCENARIO);
  const reconnectFeature = requiredFeature(reconnect.features[0], RECONNECT_SCENARIO);
  const reloadFeature = requiredFeature(reload.features[0], RELOAD_SCENARIO);
  const restartFeature = requiredFeature(restart.features[0], RESTART_SCENARIO);
  const liveCriterion = requiredCriterion(liveFeature.criteria[0], LIVE_SCENARIO);
  const reconnectCriterion = requiredCriterion(reconnectFeature.criteria[0], RECONNECT_SCENARIO);
  const reloadCriterion = requiredCriterion(reloadFeature.criteria[0], RELOAD_SCENARIO);
  const restartCriterion = requiredCriterion(restartFeature.criteria[0], RESTART_SCENARIO);

  assert.deepEqual(liveFeature.criteria.map(criterion => criterion.id), ['901a']);
  assert.deepEqual(liveCriterion.steps.map(step => step.do),
    ['dbSetStock', 'expectNumber']);
  assert.equal(liveCriterion.points, 1,
    'the candidate source and explicit recipe score must agree');
  assert.deepEqual(reloadCriterion.steps.map(step => step.do),
    ['dbSetStock', 'reload', 'expectNumber']);
  assert.equal(reloadCriterion.points, 0,
    'reload persistence is supporting evidence, not a second score');

  const reconnectSteps = reconnectCriterion.steps;
  assert.deepEqual(reconnectSteps.map(step => step.do),
    ['setOffline', 'dbSetStock', 'setOffline', 'expectNumber']);
  const disconnectedWrite = reconnectSteps[1];
  const reconnectResult = reconnectSteps.at(-1);
  assert(disconnectedWrite && reconnectResult, 'reconnect checks must contain their boundary steps');
  assert.equal(disconnectedWrite.settleMs, 4000,
    'the external write must remain inside the disconnected window before network restoration');
  assert.equal(reconnectResult.equals, 52,
    'East 7 + untouched West 45 must not depend on the server-restart scenario');
  assert.equal(reconnectSteps.some(step => ['startAppServer', 'stopAppServer'].includes(step.do)), false);
  assert.equal(reconnectCriterion.points, 1,
    'the candidate source and explicit recipe score must agree');

  const restartSteps = restartCriterion.steps;
  assert.deepEqual(restartSteps.map(step => step.do),
    ['stopAppServer', 'dbSetStock', 'startAppServer', 'expectNumber']);
  const restartResult = restartSteps.at(-1);
  assert(restartResult, 'restart checks must contain a final result');
  assert.equal(restartResult.equals, 65,
    'untouched East 55 + West 10 must not depend on the live-sync scenario');
  assert.equal(restartCriterion.points, 1,
    'the previously promoted server-restart score must be preserved');
});

test('the qualified pack preserves the existing stable check identities', () => {
  const pack = compilePackDefinition(json(PACK), { source: PACK });
  assert.equal(pack.state, 'qualified');
  assert.deepEqual(pack.checks.map(check => [check.id, check.stableId, check.criteria]), [
    ['external-live', 'external-stock', ['901a']],
    ['external-reload', 'external-stock', ['901b']],
    ['external-server-restart', 'external-stock', ['901c']],
    ['external-reconnect', 'external-stock', ['901d']],
  ]);
});

test('901b is independently selectable from the current L1 recipe', () => {
  const release = buildRecipeRelease(join(ROOT, 'tracks', 'ecommerce', 'composition', 'recipes',
    'sequential-l1-2.5.0.json'));
  const key = 'ecommerce.spec.external-data-sync.external-stock.901b';
  const check = release.checkCatalog.find(candidate => candidate.stableKey === key);
  assert(check, `${key} must exist in the release`);
  assert.equal(check.source, 'scenarios/01-external-reload-sync-1.1.0.json');
  assert.equal(check.points, 0);
  const reloadScenario = compileScenarioDefinition(json(RELOAD_SCENARIO), {
    source: RELOAD_SCENARIO,
    expectedLevel: 1,
  });
  const selected = selectScenarioChecks(reloadScenario, { checks: release.checkCatalog }, [key]);
  const feature = requiredFeature(selected.features[0], RELOAD_SCENARIO);
  const criterion = requiredCriterion(feature.criteria[0], RELOAD_SCENARIO);
  const setup = feature.setup[0];
  assert(setup, 'the selected reload scenario must have setup');
  assert.deepEqual(feature.criteria.map(item => item.id), ['901b']);
  assert.equal(setup.equals, 100);
  assert.deepEqual(criterion.steps.map(step => step.do),
    ['dbSetStock', 'reload', 'expectNumber']);
});

for (const entry of cases) {
  test(`${entry.backend} external-sync mutations are exact and fixture-bound`, () => {
    const manifest = manifestFor(entry.backend);
    const work = mkdtempSync(join(tmpdir(), `stack-bench-external-sync-${entry.backend}-`));
    try {
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend: entry.backend,
        track: 'ecommerce',
        level: 1,
        recipe: 'ecommerce.sequential-l1@2.5.0',
        app,
      });
      assert.equal(prepared.fixture.id, `ecommerce-reference-${entry.backend}`);
      assert.equal(prepared.sourceSha256, entry.fixtureSha256);
      assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        defaultScenario: manifest.scenario,
        requireScenario: true,
      }).issues, []);
      const mutations = manifest.mutations.filter(mutation =>
        mutation.targets.some(key =>
          key.startsWith('ecommerce.spec.external-data-sync.external-stock.901')));
      assert.deepEqual(mutations.map(mutation => [
        mutation.id,
        mutationScenario(manifest, mutation),
        mutation.targets,
      ]), entry.mutations);
      assert.equal(mutations.some(mutation =>
        mutation.targets.includes('ecommerce.spec.external-data-sync.external-stock.901b')), false,
      'supporting reload evidence must not acquire a synthetic mutant');

      for (const mutation of mutations) {
        const edits = mutationFileEdits(mutation);
        assert(edits.length > 0, `${mutation.id} must contain edits`);
        const file = requiredMutationFile(mutation);
        const source = readFileSync(join(app, file), 'utf8');
        for (const edit of edits) {
          assert.equal(source.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match exactly once`);
        }
        const mutated = edits
          .reduce((value, edit) => value.replace(edit.find, edit.replace), source);
        for (const edit of edits) {
          assert.equal(mutated.includes(edit.find), false,
            `${mutation.id} must remove its correct behavior`);
          assert.equal(mutated.includes(edit.replace), true,
            `${mutation.id} must install its declared defect`);
        }
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
          .map(diagnostic => ts.flattenDiagnosticMessageText(diagnostic.messageText, '\n')), []);
      }
    } finally {
      rmSync(work, { recursive: true, force: true });
    }
  });
}

test('MongoDB reconnect calibration freezes both catalog recovery paths only after going offline', () => {
  const manifest = manifestFor('mongodb');
  const mutation = requiredMutation(manifest, manifest.mutations.find(candidate =>
    candidate.id === 'reconnect-generation-ignores-current-catalog'));
  assert.deepEqual(mutation.targets, ['ecommerce.spec.external-data-sync.external-stock.901d']);
  assert.equal(mutation.file, 'client/src/App.tsx');
  const edits = mutationFileEdits(mutation);
  assert.equal(edits.length, 3);
  assert.match(requiredEdit(edits[0], mutation.id).replace, /addEventListener\("offline"/);
  assert.match(requiredEdit(edits[1], mutation.id).replace, /acceptCatalogUpdates\.current/);
  assert.match(requiredEdit(edits[2], mutation.id).replace, /acceptCatalogUpdates\.current/);
});

test('PostgreSQL reconnect calibration freezes catalog updates only after the offline boundary', () => {
  const manifest = manifestFor('postgres');
  const mutation = requiredMutation(manifest, manifest.mutations.find(candidate =>
    candidate.id === 'reconnect-does-not-send-current-catalog'));
  assert.deepEqual(mutation.targets, ['ecommerce.spec.external-data-sync.external-stock.901d']);
  assert.equal(mutation.file, 'client/src/App.tsx');
  const edits = mutationFileEdits(mutation);
  assert.match(requiredEdit(edits[0], mutation.id).replace, /addEventListener\("offline"/);
  assert.match(requiredEdit(edits[1], mutation.id).replace, /acceptCatalogUpdates\.current/);
});

test('SpacetimeDB reconnect calibration preserves the last online stock snapshot', () => {
  const manifest = manifestFor('spacetime');
  const mutation = requiredMutation(manifest, manifest.mutations.find(candidate =>
    candidate.id === 'stock-view-keeps-pre-reconnect-snapshot'));
  const replacement = requiredEdit(mutationFileEdits(mutation).at(-1), mutation.id).replace;
  assert.deepEqual(mutation.targets, ['ecommerce.spec.external-data-sync.external-stock.901d']);
  assert.equal(mutation.file, 'client/src/App.tsx');
  assert.match(replacement, /addEventListener\('offline'/);
  assert.match(replacement, /lastOnlineStocks\.current = liveStocks/);
  assert.match(replacement, /freezeStockAfterOffline \? lastOnlineStocks\.current/);
});

function manifestFor(backend: string): LoadedMutationManifest {
  const fixture = registry.fixtures.find(candidate =>
    candidate.track === 'ecommerce' && candidate.backend === backend);
  assert.equal(fixture?.mutationManifests?.length, 1);
  const manifestPath = fixture?.mutationManifests?.[0];
  assert(manifestPath);
  return readMutationManifest(join(ROOT, manifestPath));
}

function requiredMutation(
  manifest: LoadedMutationManifest,
  mutation: LoadedMutationDefinition | undefined,
): LoadedMutationDefinition {
  if (!mutation) throw new Error(`${manifest.backend} reconnect mutation is required`);
  return mutation;
}

function requiredMutationFile(mutation: LoadedMutationDefinition): string {
  if (!mutation.file) throw new Error(`${mutation.id} must have a default file`);
  return mutation.file;
}

function requiredEdit(edit: MutationFileEdit | undefined, mutationId: string): MutationFileEdit {
  if (!edit) throw new Error(`${mutationId} must contain the expected edit`);
  return edit;
}

function requiredFeature(feature: CompiledFeature | undefined, source: string): CompiledFeature {
  if (!feature) throw new Error(`${source} must contain a feature`);
  return feature;
}

function requiredCriterion(
  criterion: CompiledCriterion | undefined,
  source: string,
): CompiledCriterion {
  if (!criterion) throw new Error(`${source} must contain a criterion`);
  return criterion;
}
