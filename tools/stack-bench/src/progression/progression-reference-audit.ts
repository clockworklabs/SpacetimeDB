import { existsSync } from 'node:fs';
import { join, resolve } from 'node:path';

import { canonicalDefinitionJson } from '../composition/definition-plan.js';
import { ARTIFACT_FILE, readArtifact } from '../evidence/artifacts.js';
import type { RecipeBinding, RecipeCheck } from '../composition/recipe-release.js';
import { progressionEngine } from './progression-engine.js';
import type { ProgressionWorkAction } from './progression-engine.js';
import { gradeBundleToProgressionResult } from './grade-bundle-result.js';
import { validateProgressionInput } from './progression-definition.js';
import type { CompiledProgressionDefinition, ProgressionInput }
  from './progression-definition.js';
import { resolveProgressionRecipeAction } from './progression-recipe-selection.js';
import { readProgressionState, validateProgressionOwner } from './progression-state.js';
import type { ProgressionEvent, ProgressionTerminalOutcome } from './progression-state.js';

interface ExactRecipeIdentity {
  id: string;
  version: string;
  sha256: string;
}

interface AuditRecipeRelease {
  id: string;
  version: string;
  contentSha256: string;
  checkCatalog: RecipeCheck[];
}

interface RecipeBindingRecord {
  level: number;
  binding: RecipeBinding;
}

type RecipeBindings = Map<number, RecipeBinding> | RecipeBindingRecord[];

interface ReferenceAuditOptions {
  outputDir?: string;
  progression?: unknown;
  featureCatalogIdentity?: unknown;
  dependencyPolicyIdentity?: unknown;
  owner?: unknown;
  recipeBindings?: RecipeBindings;
  release?: unknown;
}

interface AuditGradingCheck {
  id: string;
  points: number;
}

interface AuditWorkSelection {
  promptNodeIds: string[];
  gradingNodeIds: string[];
  checks: AuditGradingCheck[];
}

interface AuditAction {
  sequence: number;
  type: ProgressionWorkAction['type'];
  level: number;
  promptNodeIds: string[];
  gradingNodeIds: string[];
  checks: number;
  selectionSha256: string;
  sourceSha256: string;
  evidence?: unknown;
  bundle: string;
  outcome: 'conclusive' | 'inconclusive';
  passed: boolean;
  nodes?: unknown;
}

type RecordedResult = NonNullable<ProgressionEvent['result']> & { sourceSha256: string };

export interface ProgressionReferenceAuditReport {
  ok: boolean;
  progression: ProgressionInput['identity'];
  recipe: ExactRecipeIdentity;
  actions: AuditAction[];
  grants: unknown[];
  terminal: ProgressionTerminalOutcome | null;
  graphOwned: {
    nodes: number;
    checks: number;
    points: number;
    coveredNodes: number;
    coveredChecks: number;
    missingNodes: string[];
    missingChecks: string[];
    complete: boolean;
  };
  finalCatalogAudit: {
    required: true;
    status: 'passed' | 'not-run';
    checks: number;
    points: number;
    zeroPointChecks: number;
    checkKeys: string[];
    additionalChecks: Array<{ stableKey: string; points: number }>;
  };
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const same = (left: unknown, right: unknown): boolean =>
  canonicalDefinitionJson(left) === canonicalDefinitionJson(right);
const HASH = /^[a-f0-9]{64}$/;

function auditRecipeRelease(release: unknown): AuditRecipeRelease {
  if (!object(release) || typeof release.id !== 'string'
    || typeof release.version !== 'string' || typeof release.contentSha256 !== 'string'
    || !HASH.test(release.contentSha256) || !Array.isArray(release.checkCatalog)) {
    throw new Error('progression reference audit requires an exact recipe release');
  }
  return release as unknown as AuditRecipeRelease;
}

function exactRecipeIdentity(release: AuditRecipeRelease): ExactRecipeIdentity {
  return { id: release.id, version: release.version, sha256: release.contentSha256 };
}

function bindingAt(recipeBindings: RecipeBindings, level: number): RecipeBinding {
  const binding = recipeBindings instanceof Map
    ? recipeBindings.get(level)
    : recipeBindings.find(item => item.level === level)?.binding;
  if (!binding?.release) throw new Error(`progression reference audit has no recipe binding for L${level}`);
  return binding;
}

function checkMap(checks: unknown[], key: 'id' | 'stableKey', label: string): Map<string, number> {
  const result = new Map<string, number>();
  for (const check of checks) {
    const id = object(check) ? check[key] : undefined;
    if (typeof id !== 'string' || !id || result.has(id)
      || !object(check) || typeof check.points !== 'number'
      || !Number.isSafeInteger(check.points) || check.points < 0) {
      throw new Error(`${label} contains an invalid or duplicate check`);
    }
    result.set(id, check.points);
  }
  return result;
}

function coverageReport(definition: CompiledProgressionDefinition, release: AuditRecipeRelease,
  coveredNodeIds: Set<string>, coveredCheckIds: Set<string>): Pick<ProgressionReferenceAuditReport,
  'graphOwned' | 'finalCatalogAudit'> {
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
  const sum = (values: Iterable<number>): number =>
    [...values].reduce((total, value) => total + value, 0);
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

function auditWorkSelection(action: ProgressionWorkAction): AuditWorkSelection {
  if (!object(action.prompt) || !Array.isArray(action.prompt.nodeIds)
    || action.prompt.nodeIds.some(id => typeof id !== 'string' || !id)
    || !object(action.grading) || !Array.isArray(action.grading.nodeIds)
    || action.grading.nodeIds.some(id => typeof id !== 'string' || !id)
    || !Array.isArray(action.grading.checks)) {
    throw new Error('progression reference audit action selection is invalid');
  }
  const checks = action.grading.checks.map(check => {
    if (!object(check) || typeof check.id !== 'string' || !check.id
      || typeof check.points !== 'number' || !Number.isSafeInteger(check.points)) {
      throw new Error('progression reference audit action check is invalid');
    }
    return { id: check.id, points: check.points };
  });
  return {
    promptNodeIds: action.prompt.nodeIds as string[],
    gradingNodeIds: action.grading.nodeIds as string[],
    checks,
  };
}

function recordedResult(event: ProgressionEvent): RecordedResult {
  if (!object(event.result) || typeof event.result.sourceSha256 !== 'string'
    || !HASH.test(event.result.sourceSha256)) {
    throw new Error('progression reference audit recorded result is invalid');
  }
  return { ...event.result, sourceSha256: event.result.sourceSha256 };
}

export function auditProgressionReferenceRun({ outputDir, progression, featureCatalogIdentity,
  dependencyPolicyIdentity, owner,
  recipeBindings, release }: ReferenceAuditOptions = {}): ProgressionReferenceAuditReport {
  if (typeof outputDir !== 'string' || !outputDir) {
    throw new Error('progression reference audit requires an output directory');
  }
  if (!(recipeBindings instanceof Map) && !Array.isArray(recipeBindings)) {
    throw new Error('progression reference audit requires recipe bindings');
  }
  const root = resolve(outputDir);
  const validatedProgression = validateProgressionInput(progression);
  const validatedOwner = validateProgressionOwner(owner, { requireWorkspace: true });
  const validatedRelease = auditRecipeRelease(release);
  const recipeIdentity = exactRecipeIdentity(validatedRelease);
  const runArtifact = readArtifact(join(root, ARTIFACT_FILE.run), { expectedKind: 'benchmark_run' });
  const stored = readProgressionState(join(root, ARTIFACT_FILE.progressionState), {
    progression: validatedProgression,
    featureCatalogIdentity,
    dependencyPolicyIdentity,
    owner: validatedOwner,
  });
  let state = progressionEngine.initialize(validatedProgression.definition);
  let attemptSequence = 0;
  const actions: AuditAction[] = [];
  const grants: unknown[] = [];
  const coveredNodeIds = new Set<string>();
  const coveredCheckIds = new Set<string>();

  for (const event of stored.state.events) {
    if (event.type === 'repairs-granted') {
      state = progressionEngine.grantRepairs(state, event.grant);
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
    const work = auditWorkSelection(action);
    const binding = bindingAt(recipeBindings, action.level);
    const boundIdentity = exactRecipeIdentity(auditRecipeRelease(binding.release));
    if (!same(boundIdentity, recipeIdentity)) {
      throw new Error(`progression L${action.level} does not use the exact audited recipe`);
    }
    const selected = resolveProgressionRecipeAction(binding, state);
    if (!('grader' in selected) || !same(selected.action, action)) {
      throw new Error(`progression attempt ${attemptSequence} action does not match its recipe selection`);
    }
    const savedResult = recordedResult(event);
    const bundlePath = join(root, 'progression',
      `attempt-${String(attemptSequence).padStart(3, '0')}`, ARTIFACT_FILE.gradeBundle);
    if (!existsSync(bundlePath)) {
      throw new Error(`progression attempt ${attemptSequence} grade bundle is missing`);
    }
    const converted = gradeBundleToProgressionResult(
      readArtifact(bundlePath, { expectedKind: 'grade_bundle' }),
      action,
      {
        owner: validatedOwner,
        runArtifact,
        featureCatalogIdentity,
        dependencyPolicyIdentity,
        selectionSha256: selected.grader.selectionSha256,
        sourceSha256: savedResult.sourceSha256,
        recipeIdentity: boundIdentity,
        sequence: attemptSequence,
      },
    );
    const recordedGrade = {
      ...converted,
      ...(savedResult.repairRegression === undefined
        ? {} : { repairRegression: savedResult.repairRegression }),
      ...(savedResult.completedRepair === undefined
        ? {} : { completedRepair: savedResult.completedRepair }),
    };
    if (!same(recordedGrade, savedResult)) {
      throw new Error(`progression attempt ${attemptSequence} recorded result differs from its grade bundle`);
    }
    const passed = converted.outcome === 'conclusive'
      && converted.nodes.every(node => node.checks.every(check => check.outcome === 'pass'));
    if (converted.outcome === 'conclusive') {
      for (const nodeId of work.gradingNodeIds) coveredNodeIds.add(nodeId);
      for (const check of work.checks) coveredCheckIds.add(check.id);
    }
    actions.push({
      sequence: attemptSequence,
      type: action.type,
      level: action.level,
      promptNodeIds: [...work.promptNodeIds],
      gradingNodeIds: [...work.gradingNodeIds],
      checks: work.checks.length,
      selectionSha256: converted.selectionSha256,
      sourceSha256: converted.sourceSha256,
      evidence: converted.evidence,
      bundle: `progression/attempt-${String(attemptSequence).padStart(3, '0')}/${ARTIFACT_FILE.gradeBundle}`,
      outcome: converted.outcome,
      passed,
      ...(converted.outcome === 'conclusive'
        ? { nodes: structuredClone(converted.nodes) } : {}),
    });
    state = progressionEngine.recordResult(state, recordedGrade);
  }

  if (!same(state, stored.state)) {
    throw new Error('progression reference audit replay differs from the stored progression state');
  }
  const coverage = coverageReport(validatedProgression.definition, validatedRelease,
    coveredNodeIds, coveredCheckIds);
  const terminal = progressionEngine.nextAction(state);
  return {
    ok: terminal.type === 'terminal' && terminal.outcome.kind === 'passed'
      && coverage.graphOwned.complete && grants.length === 0
      && coverage.finalCatalogAudit.status === 'passed'
      && actions.every(action => action.type === 'build' && action.passed),
    progression: validatedProgression.identity,
    recipe: recipeIdentity,
    actions,
    grants,
    terminal: terminal.type === 'terminal' ? terminal.outcome : null,
    ...coverage,
  };
}
