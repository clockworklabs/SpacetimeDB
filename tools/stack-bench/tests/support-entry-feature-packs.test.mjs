import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { compileProgressionDefinitionFile } from '../dist/src/progression/progression-definition.js';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const graphPath = join(trackRoot, 'progression', 'ecommerce-1.0.0.json');
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));
const readPack = name => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

const cases = [
  {
    node: 'staff-access',
    file: 'progression-staff-access-1.0.0.json',
    level: 1,
    points: 4,
    checks: [['staff-boundary', ['601a', '601b']]],
  },
  {
    node: 'support-intake',
    file: 'progression-support-intake-1.0.0.json',
    level: 1,
    points: 2,
    checks: [['ticket-create', undefined]],
  },
  {
    node: 'support-triage',
    file: 'progression-support-triage-1.0.0.json',
    level: 2,
    points: 3,
    checks: [['assignment', ['611a']], ['priority', ['611b']], ['status', ['611c']]],
  },
  {
    node: 'support-history',
    file: 'progression-support-history-1.0.0.json',
    level: 2,
    points: 4,
    checks: [['persistence', ['612a']], ['privacy', ['612b']]],
  },
];

function wholeFile(fragment) {
  assert.equal(fragment.from, undefined);
  assert.equal(fragment.until, undefined);
  return readFileSync(join(trackRoot, fragment.path), 'utf8');
}

function packCriteria(pack, expectedLevel) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
      expectedLevel,
    });
    assert.deepEqual(scenario.features.map(feature => feature.id), [check.feature],
      `${check.source} must contain only its selected feature`);
    const feature = scenario.features[0];
    const ids = check.criteria ?? feature.criteria.map(criterion => criterion.id);
    return ids.map(id => {
      const criterion = feature.criteria.find(candidate => candidate.id === id);
      assert(criterion, `${pack.id} must own ${id}`);
      return criterion;
    });
  });
}

test('staff entry and lower support packs preserve check identity and points', () => {
  for (const entry of cases) {
    const pack = readPack(entry.file);
    assert.equal(pack.state, 'draft');
    assert.deepEqual(pack.checks.map(check => [check.id, check.criteria]), entry.checks);
    assert.equal(packCriteria(pack, entry.level)
      .reduce((total, criterion) => total + criterion.points, 0), entry.points);
  }
});

test('staff entry and lower support packs use complete single-purpose files', () => {
  const packs = new Map(cases.map(entry => [entry.node, readPack(entry.file)]));
  for (const pack of packs.values()) {
    assert.equal(pack.task.requirements.length, 1);
    assert.equal(pack.task.contracts.length, 1);
    const prompt = wholeFile(pack.task.requirements[0]);
    const contract = wholeFile(pack.task.contracts[0]);
    assert.doesNotMatch(prompt,
      /framework|ORM|database|websocket|endpoint|route|reducer|testid/i);
    assert.match(contract, /`[a-z][a-z0-9-]+`/);
  }

  assert.doesNotMatch(wholeFile(packs.get('staff-access').task.requirements[0]), /support/i);
  assert.doesNotMatch(wholeFile(packs.get('support-intake').task.requirements[0]),
    /assign|priority|status|reply|order|refund/i);
  assert.doesNotMatch(wholeFile(packs.get('support-triage').task.requirements[0]),
    /reply|order|refund|customer history/i);
  assert.doesNotMatch(wholeFile(packs.get('support-history').task.requirements[0]),
    /assign|priority|reply|order|refund/i);
  assert.match(packs.get('staff-access').checks[0].source,
    /progression-staff-access-1\.0\.0\.json$/);
});

test('staff entry and lower support dependencies match their graph parents', () => {
  const sourceNodes = new Map(readJson(graphPath).nodes.map(node => [node.id, node]));
  const definition = compileProgressionDefinitionFile(graphPath, { trackRoot });
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  for (const entry of cases) {
    const pack = readPack(entry.file);
    const parents = sourceNodes.get(entry.node).dependencies
      .flatMap(dependency => sourceNodes.get(dependency.id).featureRefs).sort();
    assert.deepEqual([...pack.requiresPacks].sort(), parents,
      `${entry.node} pack dependencies must match its graph parents`);
    const node = nodes.get(entry.node);
    assert.equal(node.level, entry.level);
    assert.equal(node.gradingChecks.reduce((total, check) => total + check.points, 0),
      entry.points);
  }
});
