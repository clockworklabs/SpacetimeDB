import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition, compileRecipeFile } from '../src/composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { compileProgressionDefinitionFile } from '../src/progression/progression-definition.mjs';
import { compileProgressionGraph,
  renderProgressionGraphHtml } from '../src/progression/progression-graph.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const definitionPath = join(trackRoot, 'progression', 'ecommerce-1.0.0.json');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

function packChecks(pack) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
    });
    const feature = scenario.features.find(candidate => candidate.id === check.feature);
    const criteria = check.criteria === undefined
      ? feature.criteria
      : check.criteria.map(id => feature.criteria.find(criterion => criterion.id === id));
    return criteria.map(criterion =>
      `${pack.stableId ?? pack.id}.${check.stableId ?? check.id}.${criterion.id}`);
  });
}

test('the ecommerce progression definition is complete and calculated from its dependencies', () => {
  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  assert.equal(definition.nodes.length, 33);
  assert.deepEqual(Object.fromEntries([1, 2, 3, 4, 5].map(level => [
    level,
    definition.nodes.filter(node => node.level === level).length,
  ])), { 1: 4, 2: 7, 3: 11, 4: 6, 5: 5 });
  assert.equal(definition.questlines.length, 11);
  assert.equal(new Set(definition.nodes.flatMap(node => node.gradingChecks.map(check => check.id))).size,
    125);
  assert.equal(definition.nodes.flatMap(node => node.gradingChecks)
    .reduce((total, check) => total + check.points, 0), 251);
  assert(definition.nodes.every(node => Object.keys(node.dependencyReasons).length
    === node.dependencies.length));
});

test('every progression feature reference and scored check binds to repository data', () => {
  const packs = readdirSync(packRoot).filter(name => name.endsWith('.json')).map(name => {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    return [`${pack.id}@${pack.version}`, pack];
  });
  const packByRef = new Map(packs);
  assert.equal(packByRef.size, packs.length, 'pack id and version pairs must be unique');

  const definition = compileProgressionDefinitionFile(definitionPath, { trackRoot });
  for (const node of definition.nodes) {
    assert.deepEqual(node.promptModules, node.featureRefs);
    const owned = new Set(node.gradingChecks.map(check => check.id));
    for (const reference of node.featureRefs) {
      const pack = packByRef.get(reference);
      assert(pack, `${node.id} references missing ${reference}`);
      assert.equal(pack.moduleType, 'feature');
      assert(packChecks(pack).every(key => owned.has(key)),
        `${node.id} must own every check selected by ${reference}`);
    }
  }

  const recipe = compileRecipeFile(join(trackRoot, 'composition', 'recipes', 'l3-standard-1.0.0.json'), {
    trackRoot,
  });
  const actual = new Map(definition.nodes.flatMap(node => node.gradingChecks)
    .map(check => [check.id, check.points]));
  for (const check of recipe.checks.filter(item => item.points > 0)) {
    assert.equal(actual.get(check.stableKey), check.points,
      `progression must preserve ${check.stableKey} from the L3 candidate`);
  }
});

test('the dependency graph page is generated from the ecommerce definition', () => {
  const htmlPath = join(import.meta.dirname, '..', 'docs', 'dependency-graph.html');
  const html = readFileSync(htmlPath, 'utf8');
  const graph = compileProgressionGraph(definitionPath, { trackRoot });
  assert.equal(graph.nodes.length, 33);
  assert.equal(graph.levels, 5);
  assert.deepEqual(graph.nodes.filter(node => node.level === 1).map(node => node.id),
    ['accounts', 'catalog', 'staff-access', 'support-intake']);
  assert.equal(renderProgressionGraphHtml(html, graph), html,
    'run npm run graph after changing the progression definition');
});
