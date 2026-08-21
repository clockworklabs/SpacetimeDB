import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';

import { compilePackDefinition } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { mutationEdits, mutationScenario, mutationTargetKeys,
  validateMutationDefinitions } from '../src/evidence/mutation-analysis.mjs';
import { loadReferenceRegistry, prepareReferenceFixtureSource,
  selectReferenceFixture } from '../src/references/reference-fixtures.mjs';
import { buildRecipeRelease } from '../src/composition/recipe-release.mjs';
import { selectScenarioChecks } from '../src/composition/recipe-selection.mjs';

const ROOT = join(import.meta.dirname, '..');
const LIVE_SCENARIO = 'tracks/ecommerce/scenarios/01-external-live-sync-1.1.0.json';
const RELOAD_SCENARIO = 'tracks/ecommerce/scenarios/01-external-reload-sync-1.1.0.json';
const RESTART_SCENARIO = 'tracks/ecommerce/scenarios/01-external-server-restart-sync-1.1.0.json';
const RECONNECT_SCENARIO = 'tracks/ecommerce/scenarios/01-external-reconnect-sync-1.1.0.json';
const PACK = 'tracks/ecommerce/composition/packs/spec-external-data-sync-1.1.0.json';
const MUTATIONS = 'grader/mutations';

function prepareReferenceSource(args) {
  const fixture = selectReferenceFixture(loadReferenceRegistry(), args);
  const prepared = prepareReferenceFixtureSource(fixture, args.app);
  return { fixture, sourceSha256: prepared.sha256 };
}

const cases = [
  {
    backend: 'mongodb',
    fixtureSha256: 'd90ea9c8326202a76bf570d0eb7c716531e3e6e3eb4a4678c677783e9d5dbb40',
    mutations: [
      ['external-stock-polling-disabled', LIVE_SCENARIO, ['901:901a']],
      ['reconnect-generation-ignores-current-catalog', RECONNECT_SCENARIO, ['901:901d']],
    ],
  },
  {
    backend: 'postgres',
    fixtureSha256: 'ffc2192ee7bce1a5f5e60bd4158118f44dd0d5cc1fcf0bcf21bc38fbfb20d6f1',
    mutations: [
      ['external-stock-polling-disabled', LIVE_SCENARIO, ['901:901a']],
      ['reconnect-does-not-send-current-catalog', RECONNECT_SCENARIO, ['901:901d']],
    ],
  },
  {
    backend: 'spacetime',
    fixtureSha256: '7deedf0dc4c17064b9a6a9bb76bc0c488cd04f21472ce7412539ac98368fd3e6',
    mutations: [
      ['stock-subscription-snapshotted-once', LIVE_SCENARIO, ['901:901a']],
      ['stock-view-keeps-pre-reconnect-snapshot', RECONNECT_SCENARIO, ['901:901d']],
    ],
  },
];

function json(relative) {
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

  assert.deepEqual(live.features[0].criteria.map(criterion => criterion.id), ['901a']);
  assert.deepEqual(live.features[0].criteria[0].steps.map(step => step.do),
    ['dbSetStock', 'expectNumber']);
  assert.equal(live.features[0].criteria[0].points, 1,
    'the candidate source and explicit recipe score must agree');
  assert.deepEqual(reload.features[0].criteria[0].steps.map(step => step.do),
    ['dbSetStock', 'reload', 'expectNumber']);
  assert.equal(reload.features[0].criteria[0].points, 0,
    'reload persistence is supporting evidence, not a second score');

  const reconnectSteps = reconnect.features[0].criteria[0].steps;
  assert.deepEqual(reconnectSteps.map(step => step.do),
    ['setOffline', 'dbSetStock', 'setOffline', 'expectNumber']);
  assert.equal(reconnectSteps[1].settleMs, 4000,
    'the external write must remain inside the disconnected window before network restoration');
  assert.equal(reconnectSteps.at(-1).equals, 52,
    'East 7 + untouched West 45 must not depend on the server-restart scenario');
  assert.equal(reconnectSteps.some(step => ['startAppServer', 'stopAppServer'].includes(step.do)), false);
  assert.equal(reconnect.features[0].criteria[0].points, 1,
    'the candidate source and explicit recipe score must agree');

  const restartSteps = restart.features[0].criteria[0].steps;
  assert.deepEqual(restartSteps.map(step => step.do),
    ['stopAppServer', 'dbSetStock', 'startAppServer', 'expectNumber']);
  assert.equal(restartSteps.at(-1).equals, 65,
    'untouched East 55 + West 10 must not depend on the live-sync scenario');
  assert.equal(restart.features[0].criteria[0].points, 1,
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

test('901b is independently selectable from the L1 2.3 recipe', () => {
  const release = buildRecipeRelease(join(ROOT, 'tracks', 'ecommerce', 'composition', 'recipes',
    'l1-modular-2.3.0.json'));
  const key = 'ecommerce.spec.external-data-sync.external-stock.901b';
  const check = release.checkCatalog.find(candidate => candidate.stableKey === key);
  assert.equal(check.source, 'scenarios/01-external-reload-sync-1.1.0.json');
  assert.equal(check.points, 0);
  const selected = selectScenarioChecks(json(RELOAD_SCENARIO), { checks: release.checkCatalog }, [key]);
  assert.deepEqual(selected.features[0].criteria.map(criterion => criterion.id), ['901b']);
  assert.equal(selected.features[0].setup[0].equals, 100);
  assert.deepEqual(selected.features[0].criteria[0].steps.map(step => step.do),
    ['dbSetStock', 'reload', 'expectNumber']);
});

for (const entry of cases) {
  test(`${entry.backend} external-sync mutations are exact and fixture-bound`, () => {
    const manifestPath = join(MUTATIONS,
      `${entry.backend}-ecom-l1-modular-2.3.0.json`);
    const manifest = json(manifestPath);
    const work = mkdtempSync(join(tmpdir(), `stack-bench-external-sync-${entry.backend}-`));
    try {
      const app = join(work, 'app');
      const prepared = prepareReferenceSource({
        backend: entry.backend,
        track: 'ecommerce',
        level: 1,
        recipe: 'ecommerce.l1-modular@2.3.0',
        app,
      });
      assert.equal(prepared.fixture.id, `ecommerce-l1-direct-actions-${entry.backend}`);
      assert.equal(prepared.sourceSha256, entry.fixtureSha256);
      assert.equal(manifest.fixtureSha256, prepared.sourceSha256);
      assert.deepEqual(validateMutationDefinitions(manifest.mutations, {
        defaultScenario: manifest.scenario,
        requireScenario: true,
      }).issues, []);
      const mutations = manifest.mutations.filter(mutation =>
        mutationTargetKeys(mutation).some(key => key.startsWith('901:')));
      assert.deepEqual(mutations.map(mutation => [
        mutation.id,
        mutationScenario(manifest, mutation),
        mutationTargetKeys(mutation),
      ]), entry.mutations);
      assert.equal(mutations.some(mutation =>
        mutationTargetKeys(mutation).includes('901:901b')), false,
      'supporting reload evidence must not acquire a synthetic mutant');

      for (const mutation of mutations) {
        const source = readFileSync(join(app, mutation.file), 'utf8');
        for (const edit of mutationEdits(mutation)) {
          assert.equal(source.split(edit.find).length - 1, 1,
            `${mutation.id} anchor must match exactly once`);
        }
        const mutated = mutationEdits(mutation)
          .reduce((value, edit) => value.replace(edit.find, edit.replace), source);
        for (const edit of mutationEdits(mutation)) {
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
          fileName: mutation.file,
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
  const manifest = json(join(MUTATIONS, 'mongodb-ecom-l1-modular-2.3.0.json'));
  const mutation = manifest.mutations.find(candidate =>
    candidate.id === 'reconnect-generation-ignores-current-catalog');
  assert.deepEqual(mutationTargetKeys(mutation), ['901:901d']);
  assert.equal(mutation.file, 'client/src/App.tsx');
  assert.equal(mutationEdits(mutation).length, 3);
  assert.match(mutationEdits(mutation)[0].replace, /addEventListener\(\"offline\"/);
  assert.match(mutationEdits(mutation)[1].replace, /acceptCatalogUpdates\.current/);
  assert.match(mutationEdits(mutation)[2].replace, /acceptCatalogUpdates\.current/);
});

test('PostgreSQL reconnect calibration freezes catalog updates only after the offline boundary', () => {
  const manifest = json(join(MUTATIONS, 'postgres-ecom-l1-modular-2.3.0.json'));
  const mutation = manifest.mutations.find(candidate =>
    candidate.id === 'reconnect-does-not-send-current-catalog');
  assert.deepEqual(mutationTargetKeys(mutation), ['901:901d']);
  assert.equal(mutation.file, 'client/src/App.tsx');
  assert.match(mutationEdits(mutation)[0].replace, /addEventListener\(\"offline\"/);
  assert.match(mutationEdits(mutation)[1].replace, /acceptCatalogUpdates\.current/);
});

test('SpacetimeDB reconnect calibration preserves the last online stock snapshot', () => {
  const manifest = json(join(MUTATIONS, 'spacetime-ecom-l1-modular-2.3.0.json'));
  const mutation = manifest.mutations.find(candidate =>
    candidate.id === 'stock-view-keeps-pre-reconnect-snapshot');
  const replacement = mutationEdits(mutation).at(-1).replace;
  assert.deepEqual(mutationTargetKeys(mutation), ['901:901d']);
  assert.equal(mutation.file, 'client/src/App.tsx');
  assert.match(replacement, /addEventListener\('offline'/);
  assert.match(replacement, /lastOnlineStockRows\.current = liveStockRows/);
  assert.match(replacement, /freezeStockAfterOffline \? lastOnlineStockRows\.current/);
});
