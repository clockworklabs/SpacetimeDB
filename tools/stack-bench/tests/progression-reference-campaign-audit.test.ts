import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { resolveRecipeRelease } from '../src/composition/recipe-release.mjs';
import { auditProgressionReferenceCampaign, formatProgressionReferenceCampaignAudit }
  from '../src/campaigns/progression-reference-campaign-audit.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const campaignPath = join(STACK_BENCH_ROOT, 'appliance',
  'campaign.ecommerce-progression-reference.json');
const plan = compileCampaignFile(campaignPath);

function completedCampaign() {
  const state = { status: 'completed', attempts: plan.attempts.map(attempt => ({
    plan: attempt,
    status: 'completed',
    executions: [{
      id: `${attempt.id}-execution1`,
      output: `attempts/${attempt.id}/execution-1`,
    }],
  })) };
  return { plan, state, paths: { root: join(import.meta.dirname, 'fixtures', 'campaign-audit') } };
}

const passingAudit = {
  ok: true,
  graphOwned: {
    nodes: 39,
    checks: 146,
    points: 281,
    coveredNodes: 39,
    coveredChecks: 146,
    missingNodes: [],
    missingChecks: [],
    complete: true,
  },
  finalCatalogAudit: {
    status: 'not-run',
    checks: 151,
    points: 286,
    zeroPointChecks: 2,
    additionalChecks: [
      { stableKey: 'one', points: 1 },
      { stableKey: 'two', points: 1 },
      { stableKey: 'three', points: 1 },
      { stableKey: 'four', points: 1 },
      { stableKey: 'five', points: 1 },
    ],
  },
};

test('completed reference campaigns audit exact owners and recipe bindings', () => {
  const campaign = completedCampaign();
  const calls = [];
  const report = auditProgressionReferenceCampaign(campaign.paths.root, {
    readState: () => campaign,
    auditRun: input => {
      calls.push(input);
      const expected = plan.attempts.find(attempt => attempt.id === input.owner.attempt.id);
      assert(expected);
      assert.deepEqual(input.owner, {
        schemaVersion: 1,
        campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
        attempt: {
          id: expected.id,
          track: plan.definition.track,
          stack: expected.stack,
          agentAdapter: expected.agentAdapter,
          model: expected.model,
          conditionSha256: expected.condition.sha256,
        },
        workspace: { appDirectory: 'source' },
      });
      assert.deepEqual([...input.recipeBindings.keys()], [1, 2, 3, 4, 5]);
      assert.equal(input.release.id, 'ecommerce.progression-catalog');
      assert.equal(input.release.version, '1.0.0');
      return structuredClone(passingAudit);
    },
  });

  assert(report);
  assert.equal(calls.length, 3);
  assert.equal(report.ok, true);
  assert.deepEqual(report.attempts.map(attempt => attempt.stack).sort(),
    ['mongodb', 'postgres', 'spacetime']);
  const first = report.attempts[0];
  assert(first);
  assert.deepEqual(first.progressionGraph.checks, { covered: 146, total: 146 });
  assert.equal(first.fullRecipeCatalog.status, 'not-run');
  assert.equal(first.fullRecipeCatalog.outsideGraph.length, 5);
  assert.match(formatProgressionReferenceCampaignAudit(report),
    /graph 39\/39 nodes, 146\/146 checks, 281 points; full recipe catalog not-run, 151 checks, 286 points, 5 outside the graph/);
});

test('reference campaign audit rejects a recipe that changed after planning', () => {
  const campaign = completedCampaign();
  assert.throws(() => auditProgressionReferenceCampaign(campaign.paths.root, {
    readState: () => campaign,
    auditRun: () => { throw new Error('audit must not run'); },
    resolveRelease: (selectedTrack, level, reference) => {
      const binding = resolveRecipeRelease(selectedTrack, level, reference);
      const planned = plan.bindings.find(item => item.level === level);
      assert(planned);
      return { ...binding, release: { ...binding.release,
        contentSha256: level === 3 ? 'a'.repeat(64) : planned.recipe.contentSha256 } };
    },
  }), /recipe binding for L3 changed after planning/);
});
