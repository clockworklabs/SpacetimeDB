import { join, relative, resolve, sep } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { recipeReleaseIdentity, resolveRecipeRelease }
  from '../composition/recipe-release.mjs';
import { loadTrack } from '../composition/tracks.mjs';
import { auditProgressionReferenceRun }
  from '../progression/progression-reference-audit.mjs';
import { readCampaignState } from './campaign-scheduler.mjs';
import { compileProgressionInput, dependencyRuntimeDefinition }
  from '../progression/progression-definition.mjs';

function childPath(root, path) {
  const absoluteRoot = resolve(root);
  const absolute = resolve(absoluteRoot, path);
  const relation = relative(absoluteRoot, absolute);
  if (!relation || relation === '..' || relation.startsWith(`..${sep}`)) {
    throw new Error('reference audit execution output is outside the campaign directory');
  }
  return absolute;
}

function progressionOwner(plan, attempt) {
  return {
    schemaVersion: 1,
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    attempt: {
      id: attempt.id,
      track: plan.definition.track,
      stack: attempt.stack,
      agentAdapter: attempt.agentAdapter,
      model: attempt.model,
      conditionSha256: attempt.condition.sha256,
    },
    workspace: { appDirectory: 'source' },
  };
}

function exactRecipeBindings(plan, resolveRelease) {
  const track = loadTrack(plan.definition.track);
  const bindings = new Map();
  let release = null;
  for (const planned of plan.bindings) {
    const reference = `${planned.recipe.id}@${planned.recipe.version}`;
    const binding = resolveRelease(track, planned.level, reference);
    if (!binding || canonicalDefinitionJson(recipeReleaseIdentity(binding.release))
      !== canonicalDefinitionJson(planned.recipe)) {
      throw new Error(`reference audit recipe binding for L${planned.level} changed after planning`);
    }
    if (release !== null && (binding.release.id !== release.id
      || binding.release.version !== release.version
      || binding.release.contentSha256 !== release.contentSha256)) {
      throw new Error('reference audit requires one exact recipe release across all levels');
    }
    release ??= binding.release;
    bindings.set(planned.level, binding);
  }
  if (release === null) throw new Error('reference audit has no recipe bindings');
  return { bindings, release };
}

function summarizeAttempt(attempt, execution, audit) {
  return {
    id: attempt.id,
    stack: attempt.stack,
    execution: execution.id,
    ok: audit.ok,
    progressionGraph: {
      complete: audit.graphOwned.complete,
      nodes: { covered: audit.graphOwned.coveredNodes, total: audit.graphOwned.nodes },
      checks: { covered: audit.graphOwned.coveredChecks, total: audit.graphOwned.checks },
      points: audit.graphOwned.points,
      missingNodes: audit.graphOwned.missingNodes,
      missingChecks: audit.graphOwned.missingChecks,
    },
    fullRecipeCatalog: {
      status: audit.finalCatalogAudit.status,
      checks: audit.finalCatalogAudit.checks,
      points: audit.finalCatalogAudit.points,
      zeroPointChecks: audit.finalCatalogAudit.zeroPointChecks,
      outsideGraph: audit.finalCatalogAudit.additionalChecks,
    },
  };
}

export function auditProgressionReferenceCampaign(directory, {
  auditRun = auditProgressionReferenceRun,
  readState = readCampaignState,
  resolveRelease = resolveRecipeRelease,
} = {}) {
  const { plan, state, paths } = readState(directory, { requireCurrentInputs: false });
  if (state.status !== 'completed') {
    throw new Error('reference campaign audit requires a completed campaign');
  }
  const selected = state.attempts.filter(attempt =>
    attempt.plan.mode?.id === 'dependency'
    && attempt.plan.agentAdapter === 'reference-fixture');
  if (selected.length === 0) return null;
  if (!plan.featureCatalog || !plan.dependencyPolicy) {
    throw new Error('reference campaign audit requires a feature catalog and dependency policy');
  }
  const progression = compileProgressionInput(dependencyRuntimeDefinition(
    plan.featureCatalog, plan.dependencyPolicy));
  const { bindings, release } = exactRecipeBindings(plan, resolveRelease);
  const attempts = selected.map(attempt => {
    if (attempt.status !== 'completed') {
      throw new Error(`reference campaign attempt ${attempt.plan.id} is not completed`);
    }
    const execution = attempt.executions.at(-1);
    const audit = auditRun({
      outputDir: childPath(paths.root, execution.output),
      progression,
      featureCatalogIdentity: plan.featureCatalog.identity,
      dependencyPolicyIdentity: plan.dependencyPolicy.identity,
      owner: progressionOwner(plan, attempt.plan),
      recipeBindings: bindings,
      release,
    });
    return summarizeAttempt(attempt.plan, execution, audit);
  });
  return {
    schemaVersion: 1,
    campaign: { id: plan.id, version: plan.version, sha256: plan.contentSha256 },
    ok: attempts.every(attempt => attempt.ok),
    attempts,
  };
}

export function formatProgressionReferenceCampaignAudit(report) {
  if (report === null) return null;
  const lines = [`Reference progression audit: ${report.ok ? 'PASS' : 'FAIL'}`];
  for (const attempt of report.attempts) {
    const graph = attempt.progressionGraph;
    const catalog = attempt.fullRecipeCatalog;
    lines.push(`${attempt.stack}: graph ${graph.nodes.covered}/${graph.nodes.total} nodes, `
      + `${graph.checks.covered}/${graph.checks.total} checks, ${graph.points} points; `
      + `full recipe catalog ${catalog.status}, ${catalog.checks} checks, ${catalog.points} points, `
      + `${catalog.outsideGraph.length} outside the graph`);
  }
  return lines.join('\n');
}
