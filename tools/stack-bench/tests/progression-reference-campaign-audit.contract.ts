import assert from 'node:assert/strict';
import { join } from 'node:path';
import test from 'node:test';

import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
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
  return { plan, state, paths: { root: join(STACK_BENCH_ROOT, 'tests', 'fixtures', 'campaign-audit') } };
}

const passingAudit = {
  ok: false,
  graphOwned: {
    nodes: 43,
    checks: 146,
    points: 281,
    coveredNodes: 43,
    coveredChecks: 146,
    missingNodes: [],
    missingChecks: [],
    complete: true,
  },
  finalCatalogAudit: {
    status: 'not-run',
    checks: 148,
    points: 281,
    zeroPointChecks: 2,
    additionalChecks: [
      { stableKey: 'ecommerce.spec.external-data-sync.external-stock.901b', points: 0 },
      { stableKey: 'ecommerce.spec.concurrency-safety.restock-race.202-control', points: 0 },
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
          conditionSha256: expected.condition.contentSha256,
        },
        workspace: { appDirectory: 'source' },
      });
      assert.deepEqual([...input.recipeBindings.keys()], [1, 2, 3, 4, 5, 6]);
      assert.equal(input.release.id, 'ecommerce.progression-catalog');
      assert.match(input.release.contentSha256, /^[a-f0-9]{64}$/);
      return structuredClone(passingAudit);
    },
  });

  assert(report);
  assert.equal(calls.length, 3);
  assert.equal(report.ok, false);
  assert.deepEqual(report.attempts.map(attempt => attempt.stack).sort(),
    ['mongodb', 'postgres', 'spacetime']);
  const first = report.attempts[0];
  assert(first);
  assert.deepEqual(first.progressionGraph.checks, { covered: 146, total: 146 });
  assert.equal(first.fullRecipeCatalog.status, 'not-run');
  assert.equal(first.fullRecipeCatalog.outsideGraph.length, 2);
  assert.match(formatProgressionReferenceCampaignAudit(report),
    /Reference progression audit: INCOMPLETE/);
  assert.match(formatProgressionReferenceCampaignAudit(report),
    /graph 43\/43 nodes, 146\/146 checks, 281 points; full recipe catalog not-run, 148 checks, 281 points, 2 outside the graph/);
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
