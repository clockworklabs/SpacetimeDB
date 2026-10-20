import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import type { CompiledCampaignPlan } from '../src/campaigns/campaign-compiler.js';

const reference = join(STACK_BENCH_ROOT, 'appliance', 'campaign.ecommerce-progression-reference.json');

// Every level's scored checks, with their points, in a stable order.
function gradedChecks(plan: CompiledCampaignPlan): string[] {
  const attempt = plan.attempts[0];
  assert.ok(attempt);
  return attempt.condition.requested.levels.map(level => `L${level.level} `
    + (level.selection.scoredChecks ?? []).map(check => `${check.stableKey}=${check.points}`).sort().join(' '));
}

// Points available per questline: the denominators of the questline average.
function questlinePoints(plan: CompiledCampaignPlan): string[] {
  const catalog = plan.featureCatalog;
  assert.ok(catalog);
  const nodePoints = new Map(catalog.definition.nodes.map(node =>
    [node.id, node.gradingChecks.reduce((total, check) => total + check.points, 0)]));
  return catalog.definition.questlines.map(questline => `${questline.id}=`
    + questline.nodes.reduce((total, id) => total + (nodePoints.get(id) ?? 0), 0));
}

test('each stack compiles the reference graph to identical checks, points, and questline denominators', () => {
  const source = JSON.parse(readFileSync(reference, 'utf8')) as {
    stacks: Array<{ id: string; adapterVersion: string }>;
  };
  assert.deepEqual(source.stacks.map(stack => stack.id).sort(), ['mongodb', 'postgres', 'spacetime']);
  const dir = mkdtempSync(join(tmpdir(), 'stack-parity-'));
  try {
    const together = compileCampaignFile(reference);
    const alone = source.stacks.map(stack => {
      const file = join(dir, `${stack.id}.json`);
      writeFileSync(file, JSON.stringify({ ...source, stacks: [stack] }));
      const plan = compileCampaignFile(file);
      assert.deepEqual(plan.stacks.map(item => item.id), [stack.id]);
      return plan;
    });
    for (const plan of alone) {
      assert.deepEqual(gradedChecks(plan), gradedChecks(together));
      assert.deepEqual(questlinePoints(plan), questlinePoints(together));
    }
    const levels = together.attempts[0]!.condition.requested.levels;
    assert.equal(levels.length, 6);
    assert.ok(levels.every(level => (level.selection.scoredChecks ?? []).length > 0));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
