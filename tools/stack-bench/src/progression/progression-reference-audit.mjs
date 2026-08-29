import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.mjs';
import { readArtifact } from '../evidence/artifacts.mjs';
import { progressionEngine } from './progression-engine.mjs';
import { gradeBundleToProgressionResult } from './grade-bundle-result.mjs';
import { validateProgressionInput } from './progression-definition.js';
import { resolveProgressionRecipeAction } from './progression-recipe-selection.mjs';
import { readProgressionState, validateProgressionOwner } from './progression-state.js';

const same = (left, right) => canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
const HASH = /^[a-f0-9]{64}$/;

function exactRecipeIdentity(release) {
  if (!release || typeof release.id !== 'string' || typeof release.version !== 'string'
    || !HASH.test(release.contentSha256 ?? '') || !Array.isArray(release.checkCatalog)) {
    throw new Error('progression reference audit requires an exact recipe release');
  }
  return { id: release.id, version: release.version, sha256: release.contentSha256 };
}

function bindingAt(recipeBindings, level) {
  const binding = recipeBindings instanceof Map
    ? recipeBindings.get(level)
    : recipeBindings?.find(item => item.level === level)?.binding;
  if (!binding?.release) throw new Error(`progression reference audit has no recipe binding for L${level}`);
  return binding;
}

function checkMap(checks, key, label) {
  const result = new Map();
  for (const check of checks) {
    const id = check?.[key];
    if (typeof id !== 'string' || !id || result.has(id)
      || !Number.isSafeInteger(check.points) || check.points < 0) {
      throw new Error(`${label} contains an invalid or duplicate check`);
    }
    result.set(id, check.points);
  }
  return result;
}

function coverageReport(definition, release, coveredNodeIds, coveredCheckIds) {
  const nodes = new Set(definition.nodes.map(node => node.id));
  const graphChecks = checkMap(definition.nodes.flatMap(node => node.gradingChecks), 'id',
    'progression graph');
  const catalogChecks = checkMap(release.checkCatalog, 'stableKey', 'recipe catalog');
  for (const [id, points] of graphChecks) {
    if (!catalogChecks.has(id)) {
      throw new Error(`progression graph check ${id} is outside the recipe catalog`);
    }
    if (catalogChecks.get(id) !== points) {
      throw new Error(`progression graph points for ${id} differ from the recipe catalog`);
    }
  }
  const missingNodes = [...nodes].filter(id => !coveredNodeIds.has(id)).sort();
  const missingChecks = [...graphChecks.keys()].filter(id => !coveredCheckIds.has(id)).sort();
  const additionalChecks = [...catalogChecks].filter(([id]) => !graphChecks.has(id))
    .map(([stableKey, points]) => ({ stableKey, points }));
  const sum = values => [...values].reduce((total, value) => total + value, 0);
  return {
    graphOwned: {
      nodes: nodes.size,
      checks: graphChecks.size,
      points: sum(graphChecks.values()),
      coveredNodes: nodes.size - missingNodes.length,
      coveredChecks: graphChecks.size - missingChecks.length,
      missingNodes,
      missingChecks,
      complete: missingNodes.length === 0 && missingChecks.length === 0,
    },
    finalCatalogAudit: {
      required: true,
      status: 'not-run',
      checks: catalogChecks.size,
      points: sum(catalogChecks.values()),
      zeroPointChecks: [...catalogChecks.values()].filter(points => points === 0).length,
      checkKeys: [...catalogChecks.keys()],
      additionalChecks,
    },
  };
}

export function auditProgressionReferenceRun({ outputDir, progression, featureCatalogIdentity,
  dependencyPolicyIdentity, owner,
  recipeBindings, release } = {}) {
  const root = resolve(outputDir);
  progression = validateProgressionInput(progression);
  owner = validateProgressionOwner(owner, { requireWorkspace: true });
  const recipeIdentity = exactRecipeIdentity(release);
  const runArtifact = readArtifact(join(root, 'run.json'), { expectedKind: 'benchmark_run' });
  const stored = readProgressionState(join(root, 'progression-state.json'), {
    progression,
    featureCatalogIdentity,
    dependencyPolicyIdentity,
    owner,
  });
  let state = progressionEngine.initialize(progression.definition);
  let attemptSequence = 0;
  const actions = [];
  const grants = [];
  const coveredNodeIds = new Set();
  const coveredCheckIds = new Set();

  for (const event of stored.state.events) {
    if (event.type === 'strikes-granted') {
      state = progressionEngine.grantStrikes(state, event.grant);
      grants.push(structuredClone(event.grant));
      continue;
    }
    if (event.type !== 'attempt-recorded') {
      throw new Error(`progression reference audit cannot process event ${event.type}`);
    }
    attemptSequence += 1;
    const action = progressionEngine.nextAction(state);
    if (action.type === 'terminal') {
      throw new Error(`progression attempt ${attemptSequence} was recorded after the terminal action`);
    }
    const binding = bindingAt(recipeBindings, action.level);
    const boundIdentity = exactRecipeIdentity(binding.release);
    if (!same(boundIdentity, recipeIdentity)) {
      throw new Error(`progression L${action.level} does not use the exact audited recipe`);
    }
    const selected = resolveProgressionRecipeAction(binding, state);
    if (!same(selected.action, action)) {
      throw new Error(`progression attempt ${attemptSequence} action does not match its recipe selection`);
    }
    const bundlePath = join(root, 'progression',
      `attempt-${String(attemptSequence).padStart(3, '0')}`, 'bundle.json');
    if (!existsSync(bundlePath)) {
      throw new Error(`progression attempt ${attemptSequence} grade bundle is missing`);
    }
    const converted = gradeBundleToProgressionResult(
      readArtifact(bundlePath, { expectedKind: 'grade_bundle' }),
      action,
      {
        owner,
        runArtifact,
        featureCatalogIdentity,
        dependencyPolicyIdentity,
        selectionSha256: selected.grader.selectionSha256,
        sourceSha256: event.result.sourceSha256,
        recipeIdentity: boundIdentity,
        sequence: attemptSequence,
      },
    );
    if (!same(converted, event.result)) {
      throw new Error(`progression attempt ${attemptSequence} recorded result differs from its grade bundle`);
    }
    const passed = converted.outcome === 'conclusive'
      && converted.nodes.every(node => node.checks.every(check => check.outcome === 'pass'));
    if (converted.outcome === 'conclusive') {
      for (const nodeId of action.grading.nodeIds) coveredNodeIds.add(nodeId);
      for (const check of action.grading.checks) coveredCheckIds.add(check.id);
    }
    actions.push({
      sequence: attemptSequence,
      type: action.type,
      level: action.level,
      promptNodeIds: [...action.prompt.nodeIds],
      gradingNodeIds: [...action.grading.nodeIds],
      checks: action.grading.checks.length,
      selectionSha256: converted.selectionSha256,
      sourceSha256: converted.sourceSha256,
      evidence: converted.evidence,
      bundle: `progression/attempt-${String(attemptSequence).padStart(3, '0')}/bundle.json`,
      outcome: converted.outcome,
      passed,
      ...(converted.nodes ? { nodes: structuredClone(converted.nodes) } : {}),
    });
    state = progressionEngine.recordResult(state, converted);
  }

  if (!same(state, stored.state)) {
    throw new Error('progression reference audit replay differs from the stored progression state');
  }
  const coverage = coverageReport(progression.definition, release, coveredNodeIds, coveredCheckIds);
  const terminal = progressionEngine.nextAction(state);
  return {
    ok: terminal.type === 'terminal' && terminal.outcome.kind === 'passed'
      && coverage.graphOwned.complete && grants.length === 0
      && actions.every(action => action.type === 'build' && action.passed),
    progression: progression.identity,
    recipe: recipeIdentity,
    actions,
    grants,
    terminal: terminal.type === 'terminal' ? terminal.outcome : null,
    ...coverage,
  };
}
