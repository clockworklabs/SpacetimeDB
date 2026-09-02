import { createHash } from 'node:crypto';

import type {
  CompiledProgressionNode,
} from './progression-definition.js';
import {
  compileDependencyMode,
  DEPENDENCY_MODE_POLICY,
  DEPENDENCY_MODE_SCHEMA_VERSION,
} from './dependency-definition.js';
import type { CompiledDependencyDefinition } from './dependency-definition.js';
import {
  INCONCLUSIVE_CATEGORIES,
  scoreDependencyState,
} from './dependency-score.js';
import type {
  DependencyScore,
  InconclusiveCategory,
} from './dependency-score.js';
import type {
  ProgressionAction,
  ProgressionPolicy,
} from './progression-engine.js';
import type {
  ProgressionEvent,
  ProgressionGrant,
  ProgressionNodeState,
  ProgressionState,
  ProgressionTerminalOutcome,
} from './progression-state.js';
import { dependencyNodeState } from './dependency-state.js';
import {
  assertDependencyObject as strictObject,
  dependencyFailure as fail,
  dependencyIdentifier as identifier,
  dependencyNonEmptyString as nonEmptyString,
  dependencyPositiveInteger as positiveInteger,
  isDependencyObject as object,
} from './dependency-validation.js';

const getNodeState = dependencyNodeState;

const HASH = /^[a-f0-9]{64}$/;
const NODE_STATUSES = new Set(['locked', 'active', 'working', 'passed', 'failed', 'blocked'] as const);
const TERMINAL_OUTCOMES = new Set(['passed', 'partial', 'failed'] as const);
type CheckOutcome = 'pass' | 'fail' | 'not-run';
type StoredCheckOutcome = Exclude<CheckOutcome, 'not-run'> | 'test-system' | null;

interface SourceEvidence extends Record<string, unknown> {
  kind: 'grade_bundle';
  id: string;
  sha256: string;
}

interface ApplicationFailure extends Record<string, unknown> {
  phase: string;
  reason: string;
}

export interface DependencyPromptSelection {
  nodeIds: string[];
  featureRefs: string[];
  promptModules: string[];
}

export interface DependencyGradingSelection {
  nodeIds: string[];
  checks: Array<{ id: string; points: number; nodeId: string }>;
}

interface ResultCheck extends Record<string, unknown> {
  id: string;
  outcome: CheckOutcome;
}

interface ResultNode extends Record<string, unknown> {
  id: string;
  checks: ResultCheck[];
}

interface ResultBase extends Record<string, unknown> {
  attemptId: string;
  runId?: string;
  sourceSha256?: string;
  selectionSha256?: string;
  evidence?: SourceEvidence;
}

export interface ConclusiveResult extends ResultBase {
  outcome: 'conclusive';
  nodes: ResultNode[];
  applicationFailure?: ApplicationFailure;
  category?: never;
  reason?: never;
}

interface InconclusiveResult extends ResultBase {
  outcome: 'inconclusive';
  category: InconclusiveCategory;
  reason: string;
  nodes?: never;
  applicationFailure?: never;
}

export type DependencyResult = ConclusiveResult | InconclusiveResult;

export type DependencyStrikeGrant = ProgressionGrant;

interface AttemptRecordedEvent extends ProgressionEvent {
  sequence: number;
  type: 'attempt-recorded';
  result: DependencyResult;
  grant?: never;
}

interface StrikesGrantedEvent extends ProgressionEvent {
  sequence: number;
  type: 'strikes-granted';
  result?: never;
  grant: DependencyStrikeGrant;
}

export type DependencyEvent = AttemptRecordedEvent | StrikesGrantedEvent;

export interface DependencyState extends ProgressionState {
  definition: CompiledDependencyDefinition;
  events: DependencyEvent[];
  grants: DependencyStrikeGrant[];
}

export function dependencyRepairBudget(
  action: unknown,
  completedRepairRounds: number,
  initialGradePending = false,
): number {
  if (!object(action) || action.type === 'terminal' || !object(action.strikes)
    || !['feature', 'depth', 'banked'].includes(String(action.strikes.scope))
    || !Number.isSafeInteger(action.strikes.maxRemaining)
    || Number(action.strikes.maxRemaining) < 0
    || !Number.isSafeInteger(completedRepairRounds) || completedRepairRounds < 0) {
    throw new Error('dependency repair budget requires one valid strike action');
  }
  return Math.max(0, completedRepairRounds + Number(action.strikes.maxRemaining)
    - (initialGradePending ? 1 : 0));
}

interface DependencyStrikeState {
  definition: { nodes: Array<{ id: string; level: number }> };
  nodes: Record<string, {
    strikes: { initialBudget: number; granted: number; budget: number; used: number };
    exhaustedAtLevel: number | null;
    exhaustionReason: string | null;
  }>;
}

export function dependencyStrikeRecords(
  state: DependencyStrikeState,
  level: number,
  includedNodeIds: ReadonlySet<string> | readonly string[] = [],
) {
  const included = new Set(includedNodeIds);
  return state.definition.nodes
    .filter(node => node.level === level
      || state.nodes[node.id]?.exhaustedAtLevel === level
      || included.has(node.id))
    .map(node => {
      const nodeState = state.nodes[node.id];
      if (!nodeState) throw new Error(`progression state is missing node ${node.id}`);
      return { nodeId: node.id,
        initialBudget: nodeState.strikes.initialBudget,
        granted: nodeState.strikes.granted,
        budget: nodeState.strikes.budget,
        used: nodeState.strikes.used,
        remaining: nodeState.strikes.budget - nodeState.strikes.used,
        exhaustionReason: nodeState.exhaustionReason };
    })
    .sort((left, right) => left.nodeId.localeCompare(right.nodeId));
}

function validateSourceEvidence(value: unknown, at: string): asserts value is SourceEvidence {
  strictObject(value, at, new Set(['kind', 'id', 'sha256']));
  if (value.kind !== 'grade_bundle' || typeof value.id !== 'string' || !value.id
    || typeof value.sha256 !== 'string' || !HASH.test(value.sha256)) {
    throw new Error(`${at} must identify one grade bundle artifact`);
  }
}

function validateApplicationFailure(value: unknown, at: string): asserts value is ApplicationFailure {
  strictObject(value, at, new Set(['phase', 'reason']));
  nonEmptyString(value.phase, `${at}.phase`);
  nonEmptyString(value.reason, `${at}.reason`);
}

function uniqueStrings(value: unknown, at: string): string[] {
  if (!Array.isArray(value) || value.length === 0) {
    fail(at, 'must be a non-empty array');
  }
  const seen = new Set<string>();
  return (value as unknown[]).map((item, index) => {
    const result = nonEmptyString(item, `${at}[${index}]`);
    if (seen.has(result)) fail(`${at}[${index}]`, `duplicates ${JSON.stringify(result)}`);
    seen.add(result);
    identifier(result, `${at}[${index}]`);
    return result;
  });
}

function getDefinitionNode(definition: CompiledDependencyDefinition,
  nodeId: string): CompiledProgressionNode {
  const node = definition.nodes.find(candidate => candidate.id === nodeId);
  if (!node) throw new Error(`unknown dependency node ${nodeId}`);
  return node;
}

function featureChecksPass(state: DependencyState, node: CompiledProgressionNode): boolean {
  return node.gradingChecks.filter(check => check.role === 'feature')
    .every(check => getNodeState(state, node.id).checks[check.id] === 'pass');
}

function allChecksPass(state: DependencyState, node: CompiledProgressionNode): boolean {
  return node.gradingChecks.every(check =>
    getNodeState(state, node.id).checks[check.id] === 'pass');
}

function hasFailedCheck(state: DependencyState, node: CompiledProgressionNode): boolean {
  return Object.values(getNodeState(state, node.id).checks).includes('fail');
}

function hasFailedFeatureCheck(state: DependencyState, node: CompiledProgressionNode): boolean {
  return node.gradingChecks.some(check => check.role === 'feature'
    && getNodeState(state, node.id).checks[check.id] === 'fail');
}

function isUsable(status: ProgressionNodeState['status']): boolean {
  return status === 'working' || status === 'passed';
}

function canRepair(node: ProgressionNodeState, unchangedFailureLimit: number): boolean {
  return node.strikes.used < node.strikes.budget
    && node.unchangedFailure.count < unchangedFailureLimit;
}

function strikeBudget(definition: CompiledDependencyDefinition, level: number): number {
  const levels = Object.entries(definition.strikes.levels)
    .map(([key, budget]) => [Number(key), budget] as const);
  if (definition.strikePolicy === 'banked') {
    return levels.filter(([candidate]) => candidate <= level)
      .reduce((total, [, budget]) => total + budget, 0);
  }
  const budget = definition.strikes.levels[String(level)];
  if (budget === undefined) throw new Error(`missing strike budget for level ${level}`);
  return budget;
}

function spendStrike(state: DependencyState, failedNodeIds: ReadonlySet<string>): void {
  if (failedNodeIds.size === 0) return;
  if (state.definition.strikePolicy === 'feature') {
    failedNodeIds.forEach(nodeId => { getNodeState(state, nodeId).strikes.used += 1; });
    return;
  }
  const failedLevels = [...new Set([...failedNodeIds].map(nodeId =>
    getDefinitionNode(state.definition, nodeId).level))];
  const chargedLevels = state.definition.strikePolicy === 'banked'
    ? [Math.min(...failedLevels)] : failedLevels;
  for (const chargedLevel of chargedLevels) {
    state.definition.nodes
      .filter(node => state.definition.strikePolicy === 'banked'
        ? node.level >= chargedLevel : node.level === chargedLevel)
      .forEach(node => { getNodeState(state, node.id).strikes.used += 1; });
  }
}

function hasRepairableFailure(state: DependencyState, node: CompiledProgressionNode): boolean {
  const nodeState = getNodeState(state, node.id);
  return hasFailedCheck(state, node) && canRepair(nodeState,
    state.definition.unchangedFailureLimit);
}

function hasTestSystemCheck(state: DependencyState, node: CompiledProgressionNode): boolean {
  return Object.values(getNodeState(state, node.id).checks).includes('test-system');
}

function currentPromptNodeIds(state: DependencyState): string[] {
  if (state.phase !== 'active') return [];
  return state.definition.nodes.filter(node =>
    getNodeState(state, node.id).status === 'active'
      || (getNodeState(state, node.id).status === 'working'
        && (hasRepairableFailure(state, node) || hasTestSystemCheck(state, node))))
    .map(node => node.id).sort((left, right) => {
    const leftNode = getDefinitionNode(state.definition, left);
    const rightNode = getDefinitionNode(state.definition, right);
    return leftNode.level - rightNode.level || left.localeCompare(right);
  });
}

function hasConclusiveAttempt(state: DependencyState): boolean {
  return state.attempts.some(attempt => attempt.outcome === 'conclusive');
}

function selectedPromptNodeIds(state: DependencyState): string[] {
  const nodeIds = currentPromptNodeIds(state);
  const failed = nodeIds.filter(nodeId =>
    Object.values(getNodeState(state, nodeId).checks).includes('fail'));
  if (failed.length > 0 && hasConclusiveAttempt(state)) {
    return state.definition.repairSelection === 'batch' ? failed : [failed[0]!];
  }
  const newWork = nodeIds.filter(nodeId => {
    const node = getNodeState(state, nodeId);
    return node.status === 'active'
      && Object.values(node.checks).every(outcome => outcome === null);
  });
  if (newWork.length > 0) return newWork;
  return nodeIds;
}

function gradingNodeIds(state: DependencyState): string[] {
  if (state.definition.workSelection === 'all-at-once') {
    return state.definition.nodes.map(node => node.id);
  }
  const promptIds = selectedPromptNodeIds(state);
  const selected = new Set([
    ...promptIds,
    ...state.definition.nodes
      .filter(node => isUsable(getNodeState(state, node.id).status))
      .map(node => node.id),
  ]);
  const nodesById = new Map(state.definition.nodes.map(node => [node.id, node]));
  const includeDependencies = (nodeId: string): void => {
    const node = nodesById.get(nodeId);
    if (!node) return;
    for (const parentId of node.dependencies) {
      selected.add(parentId);
      includeDependencies(parentId);
    }
  };
  promptIds.forEach(includeDependencies);
  return [...selected].sort((left, right) => {
    const leftNode = getDefinitionNode(state.definition, left);
    const rightNode = getDefinitionNode(state.definition, right);
    return leftNode.level - rightNode.level || left.localeCompare(right);
  });
}

function checkRequirementsAvailable(state: DependencyState,
  check: CompiledProgressionNode['gradingChecks'][number],
  promptNodeIds: ReadonlySet<string>): boolean {
  const featureOwners = new Map(state.definition.nodes.flatMap(node => node.featureRefs.map(reference => [
    reference.slice(0, reference.lastIndexOf('@')), node.id,
  ] as const)));
  return (check.requiresFeatures ?? []).every(featureId => {
    const ownerId = featureOwners.get(featureId);
    return ownerId !== undefined
      && (promptNodeIds.has(ownerId) || isUsable(getNodeState(state, ownerId).status));
  });
}

function selectionFor(state: DependencyState, nodeIds: string[],
  field: 'featureRefs' | 'promptModules'): string[] {
  const selected = new Set(nodeIds);
  return state.definition.nodes.filter(node => selected.has(node.id)).flatMap(node => node[field]);
}

function selectedPromptWork(state: DependencyState): DependencyPromptSelection {
  const nodeIds = selectedPromptNodeIds(state);
  return {
    nodeIds,
    featureRefs: [...new Set(selectionFor(state, nodeIds, 'featureRefs'))].sort(),
    promptModules: [...new Set(selectionFor(state, nodeIds, 'promptModules'))].sort(),
  };
}

function selectedGradingWork(state: DependencyState): DependencyGradingSelection {
  const nodeIds = gradingNodeIds(state);
  const selected = new Set(nodeIds);
  const promptNodeIds = new Set(selectedPromptNodeIds(state));
  return {
    nodeIds,
    checks: state.definition.nodes.filter(node => selected.has(node.id)).flatMap(node =>
      node.gradingChecks.filter(check => state.definition.workSelection === 'all-at-once'
        || checkRequirementsAvailable(state, check, promptNodeIds))
        .map(check => ({ ...check, nodeId: node.id }))),
  };
}

export function promptSelection(state: ProgressionState): DependencyPromptSelection {
  const dependencyState = asDependencyState(state);
  return selectedPromptWork(dependencyState);
}

export function gradingSelection(state: ProgressionState): DependencyGradingSelection {
  const dependencyState = asDependencyState(state);
  return selectedGradingWork(dependencyState);
}

function terminalOutcome(state: DependencyState,
  reason: ProgressionTerminalOutcome['reason'], blockedLevel: number | null = null): ProgressionTerminalOutcome {
  const statuses = Object.values(state.nodes).map(node => node.status);
  const passed = statuses.filter(status => status === 'passed').length;
  const working = statuses.filter(status => status === 'working').length;
  const kind = passed === statuses.length ? 'passed'
    : passed + working > 0 ? 'partial' : 'failed';
  return { kind, reason, level: state.level,
    ...(blockedLevel === null ? {} : { blockedLevel }) };
}

function updateNodeStatus(state: DependencyState, node: CompiledProgressionNode): void {
  const nodeState = getNodeState(state, node.id);
  if (nodeState.status === 'locked' && Object.values(nodeState.checks).every(value => value === null)) {
    return;
  }
  if (state.definition.workSelection === 'progressive' && node.dependencies.some(parentId => {
    const status = getNodeState(state, parentId).status;
    return status === 'failed' || status === 'blocked';
  })) {
    nodeState.status = 'blocked';
    return;
  }
  if (state.definition.workSelection === 'progressive'
    && node.dependencies.some(parentId => !isUsable(getNodeState(state, parentId).status))) {
    nodeState.status = 'locked';
    return;
  }
  nodeState.exhaustedAtLevel = null;
  nodeState.exhaustionReason = null;
  if (featureChecksPass(state, node)) {
    nodeState.status = allChecksPass(state, node) ? 'passed' : 'working';
  } else if (hasFailedFeatureCheck(state, node)
    && nodeState.strikes.used >= nodeState.strikes.budget) {
    nodeState.status = 'failed';
    nodeState.exhaustedAtLevel = state.level;
    nodeState.exhaustionReason = 'strikes-exhausted';
  } else {
    nodeState.status = 'active';
  }
}

function blockBrokenDescendants(state: DependencyState): void {
  if (state.definition.workSelection === 'all-at-once') return;
  for (const node of state.definition.nodes) {
    const nodeState = getNodeState(state, node.id);
    if (nodeState.status === 'failed') continue;
    if (node.dependencies.some(parentId => {
      const status = getNodeState(state, parentId).status;
      return status === 'failed' || status === 'blocked';
    })) {
      nodeState.status = 'blocked';
      nodeState.exhaustedAtLevel = null;
      nodeState.exhaustionReason = null;
    } else if (node.dependencies.some(parentId =>
      !isUsable(getNodeState(state, parentId).status))) {
      nodeState.status = 'locked';
      nodeState.exhaustedAtLevel = null;
      nodeState.exhaustionReason = null;
    }
  }
}

function openLevel(state: DependencyState, level: number): void {
  let changed = state.definition.workSelection === 'progressive';
  while (changed) {
    changed = false;
    for (const node of state.definition.nodes) {
      const nodeState = getNodeState(state, node.id);
      const dependenciesReady = node.dependencies.every(parentId =>
        isUsable(getNodeState(state, parentId).status));
      const dependenciesFailed = node.dependencies.some(parentId => {
        const status = getNodeState(state, parentId).status;
        return status === 'failed' || status === 'blocked';
      });
      const before = nodeState.status;
      if (dependenciesFailed) {
        nodeState.status = 'blocked';
      } else if (dependenciesReady
        && (nodeState.status === 'locked' || nodeState.status === 'blocked')) {
        if (Object.values(nodeState.checks).every(value => value === null)) {
          nodeState.status = 'active';
        } else {
          updateNodeStatus(state, node);
        }
      }
      if (nodeState.status !== before) changed = true;
    }
  }
  const promptNodes = currentPromptNodeIds(state);
  if (promptNodes.length > 0) {
    state.phase = 'active';
    const selectedNodes = selectedPromptNodeIds(state);
    state.level = Math.max(...(state.definition.workSelection === 'all-at-once'
      ? state.definition.nodes.map(node => node.level)
      : selectedNodes.map(nodeId => getDefinitionNode(state.definition, nodeId).level)));
    state.terminalOutcome = null;
    return;
  }
  state.phase = 'terminal';
  const hasPassedNode = state.definition.nodes.some(node =>
    getNodeState(state, node.id).status === 'passed');
  state.terminalOutcome = terminalOutcome(state,
    hasPassedNode ? 'graph-complete' : 'no-unlocked-nodes',
    hasPassedNode ? null : level);
}

function initialDependencyState(definition: CompiledDependencyDefinition): DependencyState {
  const state: DependencyState = {
    schemaVersion: DEPENDENCY_MODE_SCHEMA_VERSION,
    policy: DEPENDENCY_MODE_POLICY,
    definition,
    phase: 'active',
    terminalOutcome: null,
    level: 1,
    nodes: Object.fromEntries(definition.nodes.map(node => {
      const budget = strikeBudget(definition, node.level);
      return [node.id, {
        status: definition.workSelection === 'all-at-once' ? 'active' : 'locked',
        exhaustedAtLevel: null,
        exhaustionReason: null,
        unchangedFailure: { fingerprint: null, count: 0 },
        strikes: { initialBudget: budget, granted: 0, budget, used: 0 },
        checks: Object.fromEntries(node.gradingChecks.map(check => [check.id, null])),
      }];
    })),
    attempts: [],
    grants: [],
    events: [],
  };
  openLevel(state, 1);
  return state;
}

export function initializeDependencyMode(input: unknown): DependencyState {
  return initialDependencyState(compileDependencyMode(input));
}

function assertState(input: unknown,
  compiledDefinition?: CompiledDependencyDefinition): CompiledDependencyDefinition {
  if (!object(input) || input.schemaVersion !== DEPENDENCY_MODE_SCHEMA_VERSION
    || input.policy !== DEPENDENCY_MODE_POLICY) {
    throw new Error('invalid dependency mode state');
  }
  const state = input as unknown as DependencyState;
  const definition = compiledDefinition ?? compileDependencyMode(state.definition);
  if (!['active', 'terminal'].includes(state.phase)) throw new Error('invalid dependency mode state phase');
  if (state.phase === 'active' && state.terminalOutcome !== null) {
    throw new Error('active dependency mode state cannot have a terminal outcome');
  }
  if (state.phase === 'terminal'
    && (!object(state.terminalOutcome) || !TERMINAL_OUTCOMES.has(state.terminalOutcome.kind)
      || !['graph-complete', 'no-unlocked-nodes'].includes(state.terminalOutcome.reason)
      || !Number.isInteger(state.terminalOutcome.level)
      || (state.terminalOutcome.blockedLevel !== undefined
        && !Number.isInteger(state.terminalOutcome.blockedLevel)))) {
    throw new Error('terminal dependency mode state requires a valid outcome');
  }
  if (!Number.isInteger(state.level) || !definition.strikes.levels[String(state.level)]) {
    throw new Error('invalid dependency mode state level');
  }
  const expectedNodeIds = new Set(definition.nodes.map(node => node.id));
  const actualNodeIds = Object.keys(state.nodes ?? {});
  if (actualNodeIds.length !== expectedNodeIds.size
    || actualNodeIds.some(nodeId => !expectedNodeIds.has(nodeId))) {
    throw new Error('invalid dependency mode state node set');
  }
  for (const node of definition.nodes) {
    const nodeState = getNodeState(state, node.id);
    const unchangedFailure = nodeState?.unchangedFailure;
    const strikes = nodeState?.strikes;
    const initialBudget = strikeBudget(definition, node.level);
    if (!object(nodeState) || !NODE_STATUSES.has(nodeState.status) || !object(nodeState.checks)
      || !object(unchangedFailure)
      || !object(strikes) || strikes.initialBudget !== initialBudget
      || !Number.isSafeInteger(strikes.granted) || strikes.granted < 0
      || strikes.budget !== initialBudget + strikes.granted
      || !Number.isSafeInteger(strikes.used) || strikes.used < 0
      || strikes.used > strikes.budget
      || !Number.isSafeInteger(unchangedFailure.count) || unchangedFailure.count < 0
      || (unchangedFailure.fingerprint !== null && !HASH.test(unchangedFailure.fingerprint))
      || (unchangedFailure.count === 0) !== (unchangedFailure.fingerprint === null)
      || (nodeState.exhaustionReason !== null
        && !['strikes-exhausted', 'repeated-findings'].includes(nodeState.exhaustionReason))
      || (nodeState.exhaustedAtLevel !== null
        && (!Number.isInteger(nodeState.exhaustedAtLevel)
          || !definition.strikes.levels[String(nodeState.exhaustedAtLevel)]))
      || (nodeState.status === 'failed') !== (nodeState.exhaustedAtLevel !== null)
      || (nodeState.status === 'failed') !== (nodeState.exhaustionReason !== null)) {
      throw new Error(`invalid dependency mode state for node ${node.id}`);
    }
    const expectedChecks = new Set(node.gradingChecks.map(check => check.id));
    const actualChecks = Object.keys(nodeState.checks);
    if (actualChecks.length !== expectedChecks.size
      || actualChecks.some(checkId => !expectedChecks.has(checkId))
      || actualChecks.some(checkId =>
        ![null, 'pass', 'fail', 'test-system'].includes(nodeState.checks[checkId] ?? null))) {
      throw new Error(`invalid dependency mode check state for node ${node.id}`);
    }
  }
  if (!Array.isArray(state.attempts)) throw new Error('invalid dependency mode attempt history');
  const attemptIds = new Set();
  state.attempts.forEach((attempt, index) => {
    if (!object(attempt) || typeof attempt.attemptId !== 'string' || !attempt.attemptId
      || attemptIds.has(attempt.attemptId) || !Number.isInteger(attempt.level)
      || !definition.strikes.levels[String(attempt.level)]
      || !['conclusive', 'inconclusive'].includes(attempt.outcome)
      || (attempt.sourceSha256 !== undefined && !HASH.test(attempt.sourceSha256))
      || (attempt.selectionSha256 !== undefined && !HASH.test(attempt.selectionSha256))
      || (attempt.runId !== undefined && (typeof attempt.runId !== 'string' || !attempt.runId))
      || (attempt.outcome === 'inconclusive'
        && (typeof attempt.reason !== 'string' || !attempt.reason
          || typeof attempt.category !== 'string'
          || !INCONCLUSIVE_CATEGORIES.includes(attempt.category as InconclusiveCategory)))
      || (attempt.applicationFailure !== undefined
        && attempt.outcome !== 'conclusive')) {
      throw new Error(`invalid dependency mode attempt at index ${index}`);
    }
    if (attempt.applicationFailure !== undefined) {
      validateApplicationFailure(attempt.applicationFailure,
        `attempts[${index}].applicationFailure`);
    }
    if (attempt.evidence !== undefined) validateSourceEvidence(attempt.evidence,
      `attempts[${index}].evidence`);
    attemptIds.add(attempt.attemptId);
  });
  if (!Array.isArray(state.grants) || !Array.isArray(state.events)) {
    throw new Error('invalid dependency mode continuation history');
  }
  return definition;
}

function asDependencyState(input: ProgressionState): DependencyState {
  if (input.policy !== DEPENDENCY_MODE_POLICY) {
    throw new Error(`invalid dependency mode state policy ${JSON.stringify(input.policy)}`);
  }
  return input as DependencyState;
}

function validateConclusiveResult(state: DependencyState,
  result: ConclusiveResult): Map<string, Map<string, CheckOutcome>> {
  if (!Array.isArray(result.nodes)) throw new Error('conclusive result nodes must be an array');
  const selected = selectedGradingWork(state);
  const expectedNodes = new Map(selected.nodeIds.map(nodeId => [nodeId,
    new Set(selected.checks.filter(check => check.nodeId === nodeId).map(check => check.id))]));
  const actualNodes = new Map<string, Map<string, CheckOutcome>>();
  for (const [index, nodeResult] of result.nodes.entries()) {
    strictObject(nodeResult, `result.nodes[${index}]`, new Set(['id', 'checks']));
    nonEmptyString(nodeResult.id, `result.nodes[${index}].id`);
    if (!expectedNodes.has(nodeResult.id)) throw new Error(`result includes unselected node ${nodeResult.id}`);
    if (actualNodes.has(nodeResult.id)) throw new Error(`result repeats node ${nodeResult.id}`);
    if (!Array.isArray(nodeResult.checks)) throw new Error(`result node ${nodeResult.id} checks must be an array`);
    const expectedChecks = expectedNodes.get(nodeResult.id);
    if (!expectedChecks) throw new Error(`result includes unselected node ${nodeResult.id}`);
    const checks = new Map<string, CheckOutcome>();
    for (const [checkIndex, check] of nodeResult.checks.entries()) {
      strictObject(check, `result.nodes[${index}].checks[${checkIndex}]`, new Set(['id', 'outcome']));
      nonEmptyString(check.id, `result.nodes[${index}].checks[${checkIndex}].id`);
      if (!expectedChecks.has(check.id)) {
        throw new Error(`result includes unselected check ${check.id} for ${nodeResult.id}`);
      }
      if (checks.has(check.id)) throw new Error(`result repeats check ${check.id}`);
      if (!['pass', 'fail', 'not-run'].includes(check.outcome)) {
        throw new Error(`result check ${check.id} outcome must be pass, fail, or not-run`);
      }
      checks.set(check.id, check.outcome);
    }
    const missing = [...expectedChecks].filter(checkId => !checks.has(checkId));
    if (missing.length) throw new Error(`result node ${nodeResult.id} is missing checks: ${missing.join(', ')}`);
    actualNodes.set(nodeResult.id, checks);
  }
  const missingNodes = [...expectedNodes.keys()].filter(nodeId => !actualNodes.has(nodeId));
  if (missingNodes.length) throw new Error(`result is missing nodes: ${missingNodes.join(', ')}`);
  const currentNodes = new Set(selectedPromptNodeIds(state));
  if (result.applicationFailure !== undefined) {
    validateApplicationFailure(result.applicationFailure, 'result.applicationFailure');
    for (const [nodeId, checks] of actualNodes) {
      const expectedOutcome = currentNodes.has(nodeId) ? 'fail' : 'not-run';
      if ([...checks.values()].some(outcome => outcome !== expectedOutcome)) {
        throw new Error(`application failure must mark ${nodeId} checks ${expectedOutcome}`);
      }
    }
  }
  return actualNodes;
}

function failureFingerprint(state: DependencyState, nodeIds: string[]): string {
  const failedChecks = nodeIds.map(nodeId => ({
    nodeId,
    checks: Object.entries(getNodeState(state, nodeId).checks)
      .filter(([, outcome]) => outcome === 'fail')
      .map(([checkId]) => checkId)
      .sort(),
  })).filter(node => node.checks.length > 0);
  return createHash('sha256').update(JSON.stringify(failedChecks)).digest('hex');
}

function stopUnchangedFeatures(state: DependencyState, promptedNodeIds: ReadonlySet<string>): void {
  const failed = new Set<string>();
  const failedNodes = state.definition.nodes.filter(node => {
    const nodeState = getNodeState(state, node.id);
    return promptedNodeIds.has(node.id)
      && ['active', 'working'].includes(nodeState.status)
      && hasFailedCheck(state, node);
  });
  for (const node of failedNodes) {
    const fingerprint = failureFingerprint(state, [node.id]);
    const prior = getNodeState(state, node.id).unchangedFailure;
    const count = prior.fingerprint === fingerprint ? prior.count + 1 : 1;
    getNodeState(state, node.id).unchangedFailure = {
      fingerprint,
      count,
    };
    if (count >= state.definition.unchangedFailureLimit) {
      if (hasFailedFeatureCheck(state, node)) failed.add(node.id);
    }
  }

  for (const nodeId of failed) {
    getNodeState(state, nodeId).status = 'failed';
    getNodeState(state, nodeId).exhaustedAtLevel = state.level;
    getNodeState(state, nodeId).exhaustionReason = 'repeated-findings';
  }

  // A dependent feature is blocked by a failed prerequisite. It does not
  // spend a strike or become eligible for a continuation grant of its own.
  blockBrokenDescendants(state);
}

function applyDependencyResult(inputState: DependencyState, inputResult: DependencyResult): DependencyState {
  if (inputState.phase !== 'active') throw new Error('cannot record a result after progression terminates');
  const result = structuredClone(inputResult) as DependencyResult;
  strictObject(result, 'result', new Set([
    'attemptId', 'runId', 'outcome', 'category', 'reason', 'nodes',
    'sourceSha256', 'selectionSha256', 'evidence', 'applicationFailure',
  ]));
  nonEmptyString(result.attemptId, 'result.attemptId');
  if (result.sourceSha256 !== undefined && !HASH.test(result.sourceSha256)) {
    throw new Error('result.sourceSha256 must be a SHA-256 identity');
  }
  if (result.selectionSha256 !== undefined && !HASH.test(result.selectionSha256)) {
    throw new Error('result.selectionSha256 must be a SHA-256 identity');
  }
  if (result.runId !== undefined && (typeof result.runId !== 'string' || !result.runId)) {
    throw new Error('result.runId must be a non-empty string');
  }
  if (result.evidence !== undefined) {
    validateSourceEvidence(result.evidence, 'result.evidence');
    if (!result.runId
      || result.attemptId !== `${result.runId}-progression-${inputState.attempts.length + 1}`) {
      throw new Error('result.attemptId does not match its owned progression sequence');
    }
  }
  if (inputState.attempts.some(attempt => attempt.attemptId === result.attemptId)) {
    throw new Error(`duplicate attempt id ${result.attemptId}`);
  }
  if (!['conclusive', 'inconclusive'].includes(result.outcome)) {
    throw new Error('result.outcome must be conclusive or inconclusive');
  }
  const state = structuredClone(inputState) as DependencyState;
  if (result.outcome === 'inconclusive') {
    nonEmptyString(result.reason, 'result.reason');
    if (!INCONCLUSIVE_CATEGORIES.includes(result.category)) {
      throw new Error(`result.category must be one of ${[...INCONCLUSIVE_CATEGORIES].join(', ')}`);
    }
    if (result.nodes !== undefined || result.applicationFailure !== undefined) {
      throw new Error('inconclusive results cannot contain node grades or application failures');
    }
    state.attempts.push({ attemptId: result.attemptId, level: state.level,
      outcome: result.outcome, category: result.category, reason: result.reason,
      ...(result.runId ? { runId: result.runId } : {}),
      ...(result.evidence ? { evidence: result.evidence } : {}),
      ...(result.sourceSha256 ? { sourceSha256: result.sourceSha256 } : {}),
      ...(result.selectionSha256 ? { selectionSha256: result.selectionSha256 } : {}) });
    return state;
  }
  if (result.reason !== undefined || result.category !== undefined) {
    throw new Error('conclusive results cannot contain inconclusive details');
  }

  const promptedNodeIds = new Set(selectedPromptNodeIds(inputState));
  const actual = validateConclusiveResult(state, result);
  const selectedNodeIds = gradingNodeIds(state);
  for (const nodeId of selectedNodeIds) {
    const node = getDefinitionNode(state.definition, nodeId);
    const outcomes = actual.get(nodeId);
    if (!outcomes) throw new Error(`result is missing node ${nodeId}`);
    getNodeState(state, nodeId).checks = Object.fromEntries(node.gradingChecks.map(check => {
      const outcome = outcomes.get(check.id);
      const prior = getNodeState(state, nodeId).checks[check.id] ?? null;
      if (outcome === undefined) return [check.id, prior];
      return [check.id, outcome === 'not-run' ? (prior ?? 'test-system') : outcome];
    })) as Record<string, StoredCheckOutcome>;
  }
  const failedPromptNodeIds = new Set<string>();
  for (const nodeId of selectedNodeIds) {
    if (!promptedNodeIds.has(nodeId)) continue;
    const outcomes = actual.get(nodeId);
    if (!outcomes) throw new Error(`result is missing node ${nodeId}`);
    const nodeState = getNodeState(state, nodeId);
    const hasFailedCheck = [...outcomes.values()].includes('fail');
    if (!hasFailedCheck || !['active', 'working', 'passed'].includes(nodeState.status)) continue;
    failedPromptNodeIds.add(nodeId);
  }
  spendStrike(state, failedPromptNodeIds);
  for (const nodeId of selectedNodeIds) {
    const node = getDefinitionNode(state.definition, nodeId);
    const outcomes = actual.get(nodeId);
    if (!outcomes) throw new Error(`result is missing node ${nodeId}`);
    if ([...outcomes.values()].every(outcome => outcome === 'not-run')) continue;
    updateNodeStatus(state, node);
    if (!hasFailedCheck(state, node)) {
      getNodeState(state, nodeId).unchangedFailure = { fingerprint: null, count: 0 };
    }
  }
  if (state.definition.strikePolicy !== 'feature') {
    for (const node of state.definition.nodes) {
      if (!selectedNodeIds.includes(node.id) && hasFailedCheck(state, node)) {
        updateNodeStatus(state, node);
      }
    }
  }
  blockBrokenDescendants(state);
  state.attempts.push({ attemptId: result.attemptId, level: state.level, outcome: result.outcome,
    ...(result.runId ? { runId: result.runId } : {}),
    ...(result.evidence ? { evidence: result.evidence } : {}),
    ...(result.sourceSha256 ? { sourceSha256: result.sourceSha256 } : {}),
    ...(result.selectionSha256 ? { selectionSha256: result.selectionSha256 } : {}),
    ...(result.applicationFailure
      ? { applicationFailure: structuredClone(result.applicationFailure) } : {}) });

  stopUnchangedFeatures(state, failedPromptNodeIds);
  openLevel(state, state.level);
  return state;
}

function applyStrikeGrant(inputState: DependencyState,
  inputGrant: DependencyStrikeGrant): DependencyState {
  if (inputState.phase !== 'terminal') throw new Error('strikes can be granted only after progression terminates');
  const grant = structuredClone(inputGrant) as DependencyStrikeGrant;
  strictObject(grant, 'grant', new Set(['grantId', 'level', 'nodeIds', 'strikes']));
  nonEmptyString(grant.grantId, 'grant.grantId');
  positiveInteger(grant.level, 'grant.level');
  grant.nodeIds = uniqueStrings(grant.nodeIds, 'grant.nodeIds').sort();
  positiveInteger(grant.strikes, 'grant.strikes');
  if (inputState.grants.some(item => item.grantId === grant.grantId)) {
    throw new Error(`duplicate grant id ${grant.grantId}`);
  }
  const nodesById = new Map(inputState.definition.nodes.map(node => [node.id, node]));
  const eligible = grant.nodeIds.map(nodeId => nodesById.get(nodeId));
  if (eligible.some(node => !node
    || (inputState.definition.strikePolicy !== 'feature' && node.level !== grant.level)
    || (getNodeState(inputState, node.id).status !== 'failed'
      && !(getNodeState(inputState, node.id).status === 'working'
        && !hasRepairableFailure(inputState, node))))) {
    throw new Error(`grant level ${grant.level} includes a feature without an exhausted repair budget`);
  }
  const sharedLevel = inputState.definition.strikePolicy === 'feature'
    ? grant.level : eligible[0]?.level ?? grant.level;
  const grantedNodes = inputState.definition.strikePolicy === 'feature'
    ? eligible.filter((node): node is CompiledProgressionNode => node !== undefined)
    : inputState.definition.nodes.filter(node => inputState.definition.strikePolicy === 'banked'
      ? node.level >= sharedLevel : node.level === sharedLevel);
  for (const node of grantedNodes) {
    if (!node) continue;
    if (!Number.isSafeInteger(getNodeState(inputState, node.id).strikes.budget + grant.strikes)) {
      throw new Error(`grant for feature ${node.id} exceeds the safe strike limit`);
    }
  }
  const state = structuredClone(inputState) as DependencyState;
  state.level = grant.level;
  state.phase = 'active';
  state.terminalOutcome = null;
  const reopenDescendants = new Set<string>();
  for (const node of grantedNodes) {
    const nodeState = getNodeState(state, node.id);
    if (nodeState.status === 'failed') reopenDescendants.add(node.id);
    nodeState.strikes.granted += grant.strikes;
    nodeState.strikes.budget += grant.strikes;
    nodeState.exhaustedAtLevel = null;
    nodeState.exhaustionReason = null;
    nodeState.unchangedFailure = { fingerprint: null, count: 0 };
    updateNodeStatus(state, node);
  }
  const affected = new Set(reopenDescendants);
  let changed = true;
  while (changed) {
    changed = false;
    for (const node of state.definition.nodes) {
      if (!affected.has(node.id) && node.dependencies.some(parentId => affected.has(parentId))) {
        affected.add(node.id);
        changed = true;
      }
    }
  }
  for (const node of state.definition.nodes.filter(node =>
    !reopenDescendants.has(node.id) && node.level >= grant.level && affected.has(node.id))) {
    const nodeState = getNodeState(state, node.id);
    nodeState.checks = Object.fromEntries(node.gradingChecks.map(check => [check.id, null]));
    if (nodeState.strikes.used >= nodeState.strikes.budget) {
      nodeState.status = 'failed';
      nodeState.exhaustedAtLevel = node.level;
      nodeState.exhaustionReason = 'strikes-exhausted';
    } else {
      nodeState.status = node.level === grant.level ? 'active' : 'locked';
      nodeState.exhaustedAtLevel = null;
      nodeState.exhaustionReason = null;
    }
  }
  blockBrokenDescendants(state);
  state.grants.push(grant);
  openLevel(state, grant.level);
  return state;
}

function validateEvent(event: unknown, index: number): asserts event is DependencyEvent {
  strictObject(event, `events[${index}]`, new Set(['sequence', 'type', 'result', 'grant']));
  if (event.sequence !== index + 1) throw new Error(`event sequence ${event.sequence} must be ${index + 1}`);
  if (event.type === 'attempt-recorded') {
    if (event.result === undefined || event.grant !== undefined) {
      throw new Error(`attempt event ${event.sequence} must contain only a result`);
    }
  } else if (event.type === 'strikes-granted') {
    if (event.grant === undefined || event.result !== undefined) {
      throw new Error(`grant event ${event.sequence} must contain only a grant`);
    }
  } else {
    throw new Error(`unknown dependency mode event ${JSON.stringify(event.type)}`);
  }
}

export function replayDependencyMode(inputDefinition: unknown,
  inputEvents: unknown[] = []): DependencyState {
  const definition = compileDependencyMode(inputDefinition);
  if (!Array.isArray(inputEvents)) throw new Error('dependency mode events must be an array');
  let state = initialDependencyState(definition);
  inputEvents.forEach((inputEvent, index) => {
    const event = structuredClone(inputEvent);
    validateEvent(event, index);
    state = event.type === 'attempt-recorded'
      ? applyDependencyResult(state, event.result)
      : applyStrikeGrant(state, event.grant);
    state.events.push(event);
  });
  assertState(state, definition);
  return state;
}

export function recordDependencyResult(inputState: ProgressionState,
  inputResult: unknown): DependencyState {
  const state = asDependencyState(inputState);
  const event: DependencyEvent = { sequence: state.events.length + 1,
    type: 'attempt-recorded', result: structuredClone(inputResult) as DependencyResult };
  const next = applyDependencyResult(state, event.result);
  next.events.push(event);
  return next;
}

export function grantDependencyStrikes(inputState: ProgressionState,
  inputGrant: unknown): DependencyState {
  const state = asDependencyState(inputState);
  const event: DependencyEvent = { sequence: state.events.length + 1,
    type: 'strikes-granted', grant: structuredClone(inputGrant) as DependencyStrikeGrant };
  const next = applyStrikeGrant(state, event.grant);
  next.events.push(event);
  return next;
}

export function nextDependencyAction(inputState: ProgressionState): ProgressionAction {
  const state = asDependencyState(inputState);
  if (state.phase === 'terminal') {
    if (!state.terminalOutcome) throw new Error('terminal dependency state is missing its outcome');
    return { type: 'terminal', outcome: { ...state.terminalOutcome } };
  }
  const prompt = selectedPromptWork(state);
  const grading = selectedGradingWork(state);
  const repair = prompt.nodeIds.some(nodeId =>
    Object.values(getNodeState(state, nodeId).checks).includes('fail'));
  const featureIds = prompt.nodeIds;
  const nodes = featureIds.map(nodeId => {
    const counter = getNodeState(state, nodeId).strikes;
    return { nodeId, ...counter, remaining: counter.budget - counter.used };
  });
  const maxRemaining = Math.max(0, ...nodes.map(node => node.remaining));
  const actionLevel = state.definition.workSelection === 'all-at-once' ? state.level
    : Math.max(...featureIds.map(nodeId => getDefinitionNode(state.definition, nodeId).level));
  return {
    type: repair ? 'repair' : 'build',
    level: actionLevel,
    strikes: { scope: state.definition.strikePolicy, maxRemaining, nodes },
    prompt,
    grading,
  };
}

export function activeDependencyNodes(inputState: ProgressionState): string[] {
  const state = asDependencyState(inputState);
  return currentPromptNodeIds(state);
}

export function scoreDependencyMode(inputState: ProgressionState): DependencyScore {
  const state = asDependencyState(inputState);
  return scoreDependencyState(state);
}

export const dependencyModePolicy: Readonly<ProgressionPolicy<
  CompiledDependencyDefinition,
  ProgressionState,
  ProgressionAction,
  DependencyScore,
  DependencyGradingSelection
>> = Object.freeze({
  id: DEPENDENCY_MODE_POLICY,
  compile: compileDependencyMode,
  initialize: initializeDependencyMode,
  activeNodes: activeDependencyNodes,
  promptSelection,
  gradingSelection,
  recordResult: recordDependencyResult,
  grantStrikes: grantDependencyStrikes,
  replay: replayDependencyMode,
  nextAction: nextDependencyAction,
  score: scoreDependencyMode,
});
