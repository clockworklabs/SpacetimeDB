import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { compileProgressionDefinitionFile } from '../src/progression/progression-definition.js';
import type { CompiledPackDefinition } from '../src/composition/composition-compiler.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const packRoot = join(trackRoot, 'composition', 'packs');
const graphPath = join(trackRoot, 'progression', 'ecommerce-2.0.1.json');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string) => compilePackDefinition(readJson(join(packRoot, name)), { source: name });

interface SupportCase {
  node: string;
  file: string;
  level: number;
  points: number;
  checks: readonly [id: string, criteria: readonly string[] | undefined][];
}

const cases: readonly SupportCase[] = [
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

function wholeFile(fragment: CompiledPackDefinition['task']['requirements'][number]): string {
  assert.equal(fragment.from, undefined);
  assert.equal(fragment.until, undefined);
  return readFileSync(join(trackRoot, fragment.path), 'utf8');
}

function packCriteria(pack: CompiledPackDefinition, expectedLevel: number) {
  return pack.checks.flatMap(check => {
    const scenario = compileScenarioDefinition(readJson(join(trackRoot, check.source)), {
      source: check.source,
      expectedLevel,
    });
    assert.deepEqual(scenario.features.map(feature => feature.id), [check.feature],
      `${check.source} must contain only its selected feature`);
    const feature = scenario.features[0];
    assert.ok(feature);
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
    const requirement = pack.task.requirements[0];
    const contractFragment = pack.task.contracts[0];
    assert.ok(requirement);
    assert.ok(contractFragment);
    const prompt = wholeFile(requirement);
    const contract = wholeFile(contractFragment);
    assert.doesNotMatch(prompt,
      /framework|ORM|database|websocket|endpoint|route|reducer|testid/i);
    assert.match(contract, /`[a-z][a-z0-9-]+`/);
  }

  const staffAccess = packs.get('staff-access');
  const supportIntake = packs.get('support-intake');
  const supportTriage = packs.get('support-triage');
  const supportHistory = packs.get('support-history');
  assert.ok(staffAccess);
  assert.ok(supportIntake);
  assert.ok(supportTriage);
  assert.ok(supportHistory);
  const staffRequirement = staffAccess.task.requirements[0];
  const intakeRequirement = supportIntake.task.requirements[0];
  const triageRequirement = supportTriage.task.requirements[0];
  const historyRequirement = supportHistory.task.requirements[0];
  assert.ok(staffRequirement);
  assert.ok(intakeRequirement);
  assert.ok(triageRequirement);
  assert.ok(historyRequirement);
  assert.doesNotMatch(wholeFile(staffRequirement), /support/i);
  assert.doesNotMatch(wholeFile(intakeRequirement),
    /assign|priority|status|reply|order|refund/i);
  assert.doesNotMatch(wholeFile(triageRequirement),
    /reply|order|refund|customer history/i);
  assert.doesNotMatch(wholeFile(historyRequirement),
    /assign|priority|reply|order|refund/i);
  const staffCheck = staffAccess.checks[0];
  assert.ok(staffCheck);
  assert.match(staffCheck.source,
    /progression-staff-access-1\.0\.0\.json$/);
});

test('staff entry and lower support dependencies match their graph parents', () => {
  const definition = compileProgressionDefinitionFile(graphPath, { trackRoot });
  const nodes = new Map(definition.nodes.map(node => [node.id, node]));
  for (const entry of cases) {
    const pack = readPack(entry.file);
    const node = nodes.get(entry.node);
    assert.ok(node);
    const parents = node.dependencies.flatMap(parentId => {
      const parent = nodes.get(parentId);
      assert.ok(parent, `${entry.node} parent ${parentId} is missing`);
      return parent.featureRefs;
    }).sort();
    assert.deepEqual([...pack.requiresPacks].sort(), parents,
      `${entry.node} pack dependencies must match its graph parents`);
    assert.equal(node.level, entry.level);
    assert.equal(node.gradingChecks.reduce((total, check) => total + check.points, 0),
      entry.points);
  }
});
