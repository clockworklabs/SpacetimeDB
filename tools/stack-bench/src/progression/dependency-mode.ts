import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';

import type {
  CompiledProgressionDefinition,
  CompiledProgressionNode,
  CompiledProgressionQuestline,
} from './progression-definition.js';
import type {
  ProgressionAction,
  ProgressionPolicy,
} from './progression-engine.js';
import type {
  ProgressionEvent,
  ProgressionNodeState,
  ProgressionState,
  ProgressionTerminalOutcome,
} from './progression-state.js';

export const DEPENDENCY_MODE_SCHEMA_VERSION = 3;
export const DEPENDENCY_MODE_POLICY = 'dependency-gated';
export const FEATURE_CATALOG_SCHEMA_VERSION = 1;
export const DEFAULT_UNCHANGED_FAILURE_LIMIT = 3;

const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const HASH = /^[a-f0-9]{64}$/;
const NODE_STATUSES = new Set(['locked', 'active', 'passed', 'exhausted', 'regressed', 'blocked'] as const);
const TERMINAL_OUTCOMES = new Set(['passed', 'partial', 'failed'] as const);
const INCONCLUSIVE_CATEGORIES = new Set([
  'provider_failure', 'harness_failure', 'interrupted', 'inconclusive_evidence',
] as const);

type InconclusiveCategory = 'provider_failure' | 'harness_failure' | 'interrupted'
  | 'inconclusive_evidence';
type CheckOutcome = 'pass' | 'fail' | 'not-run';
type StoredCheckOutcome = Exclude<CheckOutcome, 'not-run'> | null;

interface SourceEvidence extends Record<string, unknown> {
  kind: 'grade_bundle';
  id: string;
  sha256: string;
}

interface ApplicationFailure extends Record<string, unknown> {
  phase: string;
  reason: string;
}

interface AuthoredDependency extends Record<string, unknown> {
  id: string;
  reason: string;
}

interface AuthoredNode extends Omit<CompiledProgressionNode, 'level' | 'dependencies' | 'dependencyReasons'> {
  level?: number;
  dependencies: Array<AuthoredDependency | string>;
  dependencyReasons?: Record<string, string>;
}

interface MutableDefinition extends Record<string, unknown> {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  policy?: string;
  strikes?: { default?: number; levels?: Record<string, number> };
  unchangedFailureLimit?: number;
  nodes: AuthoredNode[];
  questlines: Array<Omit<CompiledProgressionQuestline, 'nodes'> & { nodes?: string[] }>;
}

export interface CompiledDependencyDefinition extends CompiledProgressionDefinition {
  policy: typeof DEPENDENCY_MODE_POLICY;
  strikes: { levels: Record<string, number> };
  unchangedFailureLimit: number;
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

export interface DependencyStrikeGrant extends Record<string, unknown> {
  grantId: string;
  level: number;
  nodeIds: string[];
  strikes: number;
}

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

interface PointTotals {
  passedPoints: number;
  failedPoints: number;
  gradedPoints: number;
  ungradedPoints: number;
  availablePoints: number;
}

export interface DependencyScore {
  status: 'final' | 'provisional';
  terminalOutcome: ProgressionTerminalOutcome | null;
  attempts: {
    total: number;
    inconclusive: number;
    inconclusiveByCategory: Record<InconclusiveCategory, number>;
    conclusive: number;
  };
  questlines: Array<PointTotals & {
    id: string;
    title: string;
    percentage: number | null;
    provisionalPercentage: null;
  }>;
  averagePercentage: number | null;
  uniqueChecks: PointTotals & { percentage: number | null; provisionalPercentage: null };
}

const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at: string, message: string): never => {
  throw new Error(`invalid dependency mode at ${at}: ${message}`);
};

function strictObject(value: unknown, at: string, fields: ReadonlySet<string>): asserts value is Record<string, unknown> {
  if (!object(value)) return fail(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) fail(`${at}.${key}`, 'unknown field');
  }
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

function nonEmptyString(value: unknown, at: string): string {
  if (typeof value !== 'string' || value.trim().length === 0) return fail(at, 'must be a non-empty string');
  return value;
}

function identifier(value: unknown, at: string): string {
  const result = nonEmptyString(value, at);
  if (!ID.test(result)) return fail(at, 'must contain lowercase letters, numbers, dots, dashes, or underscores');
  return result;
}

function semanticVersion(value: unknown, at: string): string {
  const result = nonEmptyString(value, at);
  if (!VERSION.test(result)) return fail(at, 'must be an exact semantic version');
  return result;
}

function positiveInteger(value: unknown, at: string): number {
  if (!Number.isSafeInteger(value) || (value as number) < 1) {
    return fail(at, 'must be a positive integer within the safe range');
  }
  return value as number;
}

function uniqueStrings(value: unknown, at: string,
  { exactRefs = false, nonEmpty = true }: { exactRefs?: boolean; nonEmpty?: boolean } = {}): string[] {
  if (!Array.isArray(value) || (nonEmpty && value.length === 0)) {
    fail(at, `must be ${nonEmpty ? 'a non-empty' : 'an'} array`);
  }
  const seen = new Set<string>();
  return (value as unknown[]).map((item, index) => {
    const result = nonEmptyString(item, `${at}[${index}]`);
    if (seen.has(result)) fail(`${at}[${index}]`, `duplicates ${JSON.stringify(result)}`);
    seen.add(result);
    if (exactRefs) {
      const split = result.lastIndexOf('@');
      if (split < 1) fail(`${at}[${index}]`, 'must be an exact id@version reference');
      identifier(result.slice(0, split), `${at}[${index}] id`);
      semanticVersion(result.slice(split + 1), `${at}[${index}] version`);
    } else {
      identifier(result, `${at}[${index}]`);
    }
    return result;
  });
}

function compileStrikeBudgets(value: unknown, levels: number[]): Record<string, number> {
  strictObject(value, 'strikes', new Set(['default', 'levels']));
  if (value.default !== undefined) positiveInteger(value.default, 'strikes.default');
  const rawLevels = value.levels ?? {};
  strictObject(rawLevels, 'strikes.levels', new Set(levels.map(String)));
  const overrides = new Map<number, number>();
  for (const [level, budget] of Object.entries(rawLevels)) {
    if (!/^[1-9]\d*$/.test(level)) fail(`strikes.levels.${level}`, 'level key must be a positive integer');
    overrides.set(Number(level), positiveInteger(budget, `strikes.levels.${level}`));
  }
  return Object.fromEntries(levels.map(level => {
    const budget = overrides.get(level) ?? value.default;
    if (budget === undefined) fail(`strikes.levels.${level}`, 'is required when no default is set');
    return [String(level), budget];
  })) as Record<string, number>;
}

function assertAcyclic(nodesById: Map<string, CompiledProgressionNode>): void {
  const visiting = new Set<string>();
  const visited = new Set<string>();
  const visit = (node: CompiledProgressionNode, chain: string[]): void => {
    if (visited.has(node.id)) return;
    if (visiting.has(node.id)) fail('nodes', `dependency cycle: ${[...chain, node.id].join(' -> ')}`);
    visiting.add(node.id);
    for (const dependency of node.dependencies) {
      const parent = nodesById.get(dependency);
      if (parent) visit(parent, [...chain, node.id]);
    }
    visiting.delete(node.id);
    visited.add(node.id);
  };
  for (const node of nodesById.values()) visit(node, []);
}

function compileGraphDefinition(input: unknown,
  { source, catalogOnly }: { source: string; catalogOnly: boolean }): CompiledProgressionDefinition {
  const definition = structuredClone(input) as MutableDefinition;
  const fields = ['schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'nodes',
    'questlines'];
  if (!catalogOnly) fields.push('policy', 'strikes', 'unchangedFailureLimit');
  strictObject(definition, source, new Set(fields));
  const expectedSchema = catalogOnly ? FEATURE_CATALOG_SCHEMA_VERSION : DEPENDENCY_MODE_SCHEMA_VERSION;
  const expectedKind = catalogOnly ? 'feature-catalog' : 'progression-mode';
  if (definition.schemaVersion !== expectedSchema) {
    fail(`${source}.schemaVersion`, `must be ${expectedSchema}`);
  }
  if (definition.kind !== expectedKind) fail(`${source}.kind`, `must be ${JSON.stringify(expectedKind)}`);
  identifier(definition.id, `${source}.id`);
  semanticVersion(definition.version, `${source}.version`);
  if (definition.state !== 'draft' && definition.state !== 'qualified') {
    fail(`${source}.state`, 'must be "draft" or "qualified"');
  }
  nonEmptyString(definition.title, `${source}.title`);
  if (!catalogOnly && definition.policy !== DEPENDENCY_MODE_POLICY) {
    fail(`${source}.policy`, `must be ${JSON.stringify(DEPENDENCY_MODE_POLICY)}`);
  }
  if (!catalogOnly) {
    definition.unchangedFailureLimit = positiveInteger(
      definition.unchangedFailureLimit ?? DEFAULT_UNCHANGED_FAILURE_LIMIT,
      `${source}.unchangedFailureLimit`,
    );
  }
  if (!Array.isArray(definition.nodes) || definition.nodes.length === 0) {
    fail(`${source}.nodes`, 'must be a non-empty array');
  }

  const authoredNodes = structuredClone(definition.nodes);
  const nodeIds = new Set<string>();
  const checkIds = new Set<string>();
  definition.nodes.forEach((node, index) => {
    const at = `${source}.nodes[${index}]`;
    strictObject(node, at, new Set([
      'id', 'title', 'questline', 'level', 'featureRefs', 'promptModules', 'gradingChecks',
      'dependencies', 'dependencyReasons',
    ]));
    identifier(node.id, `${at}.id`);
    if (nodeIds.has(node.id)) fail(`${at}.id`, `duplicates ${JSON.stringify(node.id)}`);
    nodeIds.add(node.id);
    nonEmptyString(node.title, `${at}.title`);
    identifier(node.questline, `${at}.questline`);
    node.featureRefs = uniqueStrings(node.featureRefs, `${at}.featureRefs`, { exactRefs: true }).sort();
    node.promptModules = uniqueStrings(node.promptModules, `${at}.promptModules`,
      { exactRefs: true, nonEmpty: false }).sort();
    if (!Array.isArray(node.gradingChecks) || node.gradingChecks.length === 0) {
      fail(`${at}.gradingChecks`, 'must be a non-empty array');
    }
    const localChecks = new Set<string>();
    node.gradingChecks.forEach((check, checkIndex) => {
      const checkAt = `${at}.gradingChecks[${checkIndex}]`;
      strictObject(check, checkAt, new Set(['id', 'points']));
      identifier(check.id, `${checkAt}.id`);
      if (localChecks.has(check.id)) fail(`${checkAt}.id`, `duplicates ${JSON.stringify(check.id)}`);
      if (checkIds.has(check.id)) fail(`${checkAt}.id`, `is already owned by another node`);
      localChecks.add(check.id);
      checkIds.add(check.id);
      positiveInteger(check.points, `${checkAt}.points`);
    });
    node.gradingChecks.sort((left, right) => left.id.localeCompare(right.id));
    if (!Array.isArray(node.dependencies)) fail(`${at}.dependencies`, 'must be an array');
    const compiledNode = node.level !== undefined || node.dependencyReasons !== undefined;
    if (compiledNode && (node.level === undefined || node.dependencyReasons === undefined)) {
      fail(at, 'compiled level and dependency reasons must appear together');
    }
    if (compiledNode) positiveInteger(node.level, `${at}.level`);
    const dependencies = new Set<string>();
    node.dependencies.forEach((dependency, dependencyIndex) => {
      const dependencyAt = `${at}.dependencies[${dependencyIndex}]`;
      let dependencyId: string;
      if (compiledNode) {
        dependencyId = identifier(dependency, dependencyAt);
      }
      else {
        strictObject(dependency, dependencyAt, new Set(['id', 'reason']));
        dependencyId = identifier(dependency.id, `${dependencyAt}.id`);
        nonEmptyString(dependency.reason, `${dependencyAt}.reason`);
      }
      if (dependencies.has(dependencyId)) {
        fail(compiledNode ? dependencyAt : `${dependencyAt}.id`,
          `duplicates ${JSON.stringify(dependencyId)}`);
      }
      dependencies.add(dependencyId);
    });
    const compiledDependencies = [...dependencies].sort();
    node.dependencies = compiledDependencies;
    if (compiledNode) {
      strictObject(node.dependencyReasons, `${at}.dependencyReasons`, new Set(compiledDependencies));
      for (const dependencyId of compiledDependencies) {
        const reason = nonEmptyString(node.dependencyReasons[dependencyId],
          `${at}.dependencyReasons.${dependencyId}`);
        node.dependencyReasons[dependencyId] = reason.trim();
      }
    } else {
      const authoredDependencies = authoredNodes[index]?.dependencies ?? [];
      node.dependencyReasons = Object.fromEntries(compiledDependencies.map(dependencyId => [
        dependencyId,
        (() => {
          const dependency = authoredDependencies.find((item): item is AuthoredDependency =>
            typeof item !== 'string' && item.id === dependencyId);
          if (!dependency) return fail(`${at}.dependencies`, `missing ${dependencyId}`);
          return dependency.reason.trim();
        })(),
      ]));
    }
  });

  const nodesById = new Map(definition.nodes.map(node => [node.id, node]));
  for (const node of definition.nodes) {
    const at = `${source}.nodes.${node.id}.dependencies`;
    for (const dependency of node.dependencies) {
      if (!nodesById.has(dependency as string)) fail(at, `unknown parent ${JSON.stringify(dependency)}`);
    }
  }
  assertAcyclic(nodesById as Map<string, CompiledProgressionNode>);
  const declaredLevels = new Map(definition.nodes
    .filter(node => node.level !== undefined).map(node => [node.id, node.level]));
  definition.nodes.forEach(node => { delete node.level; });
  const levelFor = (node: AuthoredNode): number => {
    if (node.level !== undefined) return node.level;
    node.level = node.dependencies.length === 0
      ? 1
      : 1 + Math.max(...node.dependencies.map(parentId => {
        const parent = nodesById.get(parentId as string);
        if (!parent) return fail(`${source}.nodes.${node.id}.dependencies`, 'contains an unknown parent');
        return levelFor(parent);
      }));
    return node.level;
  };
  definition.nodes.forEach(levelFor);
  for (const [nodeId, declaredLevel] of declaredLevels) {
    if (nodesById.get(nodeId)?.level !== declaredLevel) {
      fail(`${source}.nodes.${nodeId}.level`, 'does not match calculated dependency depth');
    }
  }
  definition.nodes.sort((left, right) => (left.level ?? 0) - (right.level ?? 0)
    || left.id.localeCompare(right.id));
  const levels = [...new Set(definition.nodes.map(node => node.level as number))].sort((a, b) => a - b);
  if (!catalogOnly) {
    definition.strikes = { levels: compileStrikeBudgets(definition.strikes, levels) };
  }

  if (!Array.isArray(definition.questlines) || definition.questlines.length === 0) {
    fail(`${source}.questlines`, 'must be a non-empty array');
  }
  const questlineIds = new Set<string>();
  definition.questlines.forEach((questline, index) => {
    const at = `${source}.questlines[${index}]`;
    strictObject(questline, at, new Set(['id', 'title', 'nodes']));
    identifier(questline.id, `${at}.id`);
    if (questlineIds.has(questline.id)) fail(`${at}.id`, `duplicates ${JSON.stringify(questline.id)}`);
    questlineIds.add(questline.id);
    nonEmptyString(questline.title, `${at}.title`);
  });
  for (const node of definition.nodes) {
    if (!questlineIds.has(node.questline)) {
      fail(`${source}.nodes.${node.id}.questline`, `unknown questline ${JSON.stringify(node.questline)}`);
    }
  }
  definition.questlines.forEach(questline => {
    const owned = definition.nodes.filter(node => node.questline === questline.id)
      .map(node => node.id);
    if (owned.length === 0) {
      fail(`${source}.questlines.${questline.id}`, 'owns no nodes');
    }
    if (questline.nodes !== undefined) {
      const declared = uniqueStrings(questline.nodes, `${source}.questlines.${questline.id}.nodes`);
      if (!isDeepStrictEqual(declared, owned)) {
        fail(`${source}.questlines.${questline.id}.nodes`, 'does not match node ownership');
      }
    }
    questline.nodes = owned;
  });
  definition.questlines.sort((left, right) => left.id.localeCompare(right.id));
  return definition as unknown as CompiledProgressionDefinition;
}

export function compileFeatureCatalog(input: unknown,
  { source = '<feature-catalog>' }: { source?: string } = {}): CompiledProgressionDefinition {
  return compileGraphDefinition(input, { source, catalogOnly: true });
}

export function compileDependencyMode(input: unknown,
  { source = '<dependency-mode>' }: { source?: string } = {}): CompiledDependencyDefinition {
  return compileGraphDefinition(input, { source, catalogOnly: false }) as CompiledDependencyDefinition;
}

function nodesAt(definition: CompiledDependencyDefinition, level: number): CompiledProgressionNode[] {
  return definition.nodes.filter(node => node.level === level);
}

function getNodeState(state: ProgressionState, nodeId: string): ProgressionNodeState {
  const node = state.nodes[nodeId];
  if (!node) throw new Error(`dependency mode state is missing node ${nodeId}`);
  return node;
}

function getDefinitionNode(definition: CompiledDependencyDefinition,
  nodeId: string): CompiledProgressionNode {
  const node = definition.nodes.find(candidate => candidate.id === nodeId);
  if (!node) throw new Error(`unknown dependency node ${nodeId}`);
  return node;
}

function currentPromptNodeIds(state: DependencyState): string[] {
  if (state.phase !== 'active') return [];
  const current = nodesAt(state.definition, state.level)
    .filter(node => ['active', 'regressed'].includes(getNodeState(state, node.id).status))
    .map(node => node.id);
  const required = new Set([
    ...current,
    ...state.definition.nodes.filter(node => node.level < state.level
      && getNodeState(state, node.id).status === 'regressed').map(node => node.id),
  ]);
  const nodesById = new Map(state.definition.nodes.map(node => [node.id, node]));
  const includeRegressedDependencies = (nodeId: string): void => {
    const node = nodesById.get(nodeId);
    if (!node) return;
    for (const parentId of node.dependencies) {
      if (getNodeState(state, parentId).status === 'regressed') required.add(parentId);
      includeRegressedDependencies(parentId);
    }
  };
  current.forEach(includeRegressedDependencies);
  return [...required].sort((left, right) => {
    const leftNode = getDefinitionNode(state.definition, left);
    const rightNode = getDefinitionNode(state.definition, right);
    return leftNode.level - rightNode.level || left.localeCompare(right);
  });
}

function gradingNodeIds(state: DependencyState): string[] {
  const promptIds = currentPromptNodeIds(state);
  const selected = new Set([
    ...promptIds,
    ...state.definition.nodes
      .filter(node => node.level <= state.level && getNodeState(state, node.id).status === 'passed')
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

function selectionFor(state: DependencyState, nodeIds: string[],
  field: 'featureRefs' | 'promptModules'): string[] {
  const selected = new Set(nodeIds);
  return state.definition.nodes.filter(node => selected.has(node.id)).flatMap(node => node[field]);
}

function selectedPromptWork(state: DependencyState): DependencyPromptSelection {
  const nodeIds = currentPromptNodeIds(state);
  return {
    nodeIds,
    featureRefs: [...new Set(selectionFor(state, nodeIds, 'featureRefs'))].sort(),
    promptModules: [...new Set(selectionFor(state, nodeIds, 'promptModules'))].sort(),
  };
}

function selectedGradingWork(state: DependencyState): DependencyGradingSelection {
  const nodeIds = gradingNodeIds(state);
  const selected = new Set(nodeIds);
  return {
    nodeIds,
    checks: state.definition.nodes.filter(node => selected.has(node.id)).flatMap(node =>
      node.gradingChecks.map(check => ({ ...check, nodeId: node.id }))),
  };
}

export function promptSelection(state: ProgressionState): DependencyPromptSelection {
  const dependencyState = asDependencyState(state);
  assertReplayConsistent(dependencyState);
  return selectedPromptWork(dependencyState);
}

export function gradingSelection(state: ProgressionState): DependencyGradingSelection {
  const dependencyState = asDependencyState(state);
  assertReplayConsistent(dependencyState);
  return selectedGradingWork(dependencyState);
}

function terminalOutcome(state: DependencyState,
  reason: ProgressionTerminalOutcome['reason'], blockedLevel: number | null = null): ProgressionTerminalOutcome {
  const statuses = Object.values(state.nodes).map(node => node.status);
  const passed = statuses.filter(status => status === 'passed').length;
  const kind = passed === statuses.length ? 'passed' : passed > 0 ? 'partial' : 'failed';
  return { kind, reason, level: state.level,
    ...(blockedLevel === null ? {} : { blockedLevel }) };
}

function openLevel(state: DependencyState, level: number): void {
  const nodes = nodesAt(state.definition, level);
  if (nodes.length === 0) {
    state.phase = 'terminal';
    state.terminalOutcome = terminalOutcome(state, 'graph-complete');
    return;
  }
  let active = 0;
  let passed = 0;
  for (const node of nodes) {
    const unlocked = node.dependencies.every(parentId => getNodeState(state, parentId).status === 'passed');
    if (getNodeState(state, node.id).status === 'exhausted') continue;
    if (getNodeState(state, node.id).status === 'passed' && unlocked) {
      passed += 1;
      continue;
    }
    getNodeState(state, node.id).status = unlocked ? 'active' : 'blocked';
    if (unlocked) active += 1;
  }
  if (active > 0) {
    state.level = level;
    state.phase = 'active';
    state.terminalOutcome = null;
  } else if (passed > 0) {
    openLevel(state, level + 1);
  } else {
    state.phase = 'terminal';
    state.terminalOutcome = terminalOutcome(state, 'no-unlocked-nodes', level);
  }
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
      const budget = definition.strikes.levels[String(node.level)];
      if (budget === undefined) throw new Error(`missing strike budget for level ${node.level}`);
      return [node.id, {
        status: 'locked',
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

function assertState(input: unknown): CompiledDependencyDefinition {
  if (!object(input) || input.schemaVersion !== DEPENDENCY_MODE_SCHEMA_VERSION
    || input.policy !== DEPENDENCY_MODE_POLICY) {
    throw new Error('invalid dependency mode state');
  }
  const state = input as unknown as DependencyState;
  const definition = compileDependencyMode(state.definition);
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
    const initialBudget = definition.strikes.levels[String(node.level)];
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
      || (nodeState.status === 'exhausted') !== (nodeState.exhaustedAtLevel !== null)
      || (nodeState.status === 'exhausted') !== (nodeState.exhaustionReason !== null)) {
      throw new Error(`invalid dependency mode state for node ${node.id}`);
    }
    const expectedChecks = new Set(node.gradingChecks.map(check => check.id));
    const actualChecks = Object.keys(nodeState.checks);
    if (actualChecks.length !== expectedChecks.size
      || actualChecks.some(checkId => !expectedChecks.has(checkId))
      || actualChecks.some(checkId =>
        ![null, 'pass', 'fail'].includes(nodeState.checks[checkId] ?? null))) {
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
          || !INCONCLUSIVE_CATEGORIES.has(attempt.category as InconclusiveCategory)))
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

function asDependencyState(input: unknown): DependencyState {
  assertState(input);
  return input as DependencyState;
}

function validateConclusiveResult(state: DependencyState,
  result: ConclusiveResult): Map<string, Map<string, CheckOutcome>> {
  if (!Array.isArray(result.nodes)) throw new Error('conclusive result nodes must be an array');
  const nodeIds = gradingNodeIds(state);
  const selectedNodes = new Set(nodeIds);
  const selected = {
    nodeIds,
    checks: state.definition.nodes.filter(node => selectedNodes.has(node.id)).flatMap(node =>
      node.gradingChecks.map(check => ({ ...check, nodeId: node.id }))),
  };
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
  const currentNodes = new Set(currentPromptNodeIds(state));
  const hasNotRun = [...actualNodes.values()].some(checks => [...checks.values()].includes('not-run'));
  if (result.applicationFailure === undefined && hasNotRun) {
    throw new Error('not-run checks require a typed application failure');
  }
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

function stopUnchangedFeatures(state: DependencyState): void {
  const exhausted = new Set<string>();
  const failedNodes = state.definition.nodes.filter(node => {
    const nodeState = getNodeState(state, node.id);
    return ['active', 'regressed'].includes(nodeState.status)
      && Object.values(nodeState.checks).includes('fail');
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
      exhausted.add(node.id);
    }
  }

  for (const nodeId of exhausted) {
    getNodeState(state, nodeId).status = 'exhausted';
    getNodeState(state, nodeId).exhaustedAtLevel = state.level;
    getNodeState(state, nodeId).exhaustionReason = 'repeated-findings';
  }

  // A dependent feature is blocked by an exhausted prerequisite. It does not
  // spend a strike or become eligible for a continuation grant of its own.
  let changed = true;
  while (changed) {
    changed = false;
    for (const node of state.definition.nodes) {
      const nodeState = getNodeState(state, node.id);
      if (!['active', 'regressed'].includes(nodeState.status)) continue;
      if (node.dependencies.some(parentId => ['exhausted', 'blocked'].includes(getNodeState(state, parentId).status))) {
        nodeState.status = 'blocked';
        nodeState.exhaustedAtLevel = null;
        nodeState.exhaustionReason = null;
        changed = true;
      }
    }
  }
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
    if (!INCONCLUSIVE_CATEGORIES.has(result.category)) {
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

  const actual = validateConclusiveResult(state, result);
  const selectedNodeIds = gradingNodeIds(state);
  for (const nodeId of selectedNodeIds) {
    const node = getDefinitionNode(state.definition, nodeId);
    const outcomes = actual.get(nodeId);
    if (!outcomes) throw new Error(`result is missing node ${nodeId}`);
    getNodeState(state, nodeId).checks = Object.fromEntries(node.gradingChecks.map(check => {
      const outcome = outcomes.get(check.id);
      if (outcome === undefined) throw new Error(`result is missing check ${check.id}`);
      const prior = getNodeState(state, nodeId).checks[check.id] ?? null;
      return [check.id, outcome === 'not-run' ? prior : outcome];
    })) as Record<string, StoredCheckOutcome>;
  }
  for (const nodeId of selectedNodeIds) {
    const node = getDefinitionNode(state.definition, nodeId);
    const outcomes = actual.get(nodeId);
    if (!outcomes) throw new Error(`result is missing node ${nodeId}`);
    if ([...outcomes.values()].every(outcome => outcome === 'not-run')) continue;
    const checksPass = node.gradingChecks.every(check => getNodeState(state, nodeId).checks[check.id] === 'pass');
    const dependenciesPass = node.dependencies.every(parentId =>
      getNodeState(state, parentId).status === 'passed');
    getNodeState(state, nodeId).exhaustedAtLevel = null;
    getNodeState(state, nodeId).exhaustionReason = null;
    if (checksPass && dependenciesPass) {
      getNodeState(state, nodeId).status = 'passed';
      getNodeState(state, nodeId).unchangedFailure = { fingerprint: null, count: 0 };
    } else if (node.level < state.level || getNodeState(state, nodeId).status === 'passed') {
      getNodeState(state, nodeId).status = 'regressed';
    } else {
      getNodeState(state, nodeId).status = 'active';
    }
  }
  for (const nodeId of selectedNodeIds) {
    const node = getDefinitionNode(state.definition, nodeId);
    const nodeState = getNodeState(state, nodeId);
    const hasFailedCheck = node.gradingChecks.some(check => nodeState.checks[check.id] === 'fail');
    if (!hasFailedCheck || !['active', 'regressed'].includes(nodeState.status)) continue;
    nodeState.strikes.used += 1;
    if (nodeState.strikes.used >= nodeState.strikes.budget) {
      nodeState.status = 'exhausted';
      nodeState.exhaustedAtLevel = state.level;
      nodeState.exhaustionReason = 'strikes-exhausted';
    }
  }
  state.attempts.push({ attemptId: result.attemptId, level: state.level, outcome: result.outcome,
    ...(result.runId ? { runId: result.runId } : {}),
    ...(result.evidence ? { evidence: result.evidence } : {}),
    ...(result.sourceSha256 ? { sourceSha256: result.sourceSha256 } : {}),
    ...(result.selectionSha256 ? { selectionSha256: result.selectionSha256 } : {}),
    ...(result.applicationFailure
      ? { applicationFailure: structuredClone(result.applicationFailure) } : {}) });

  let unresolved = nodesAt(state.definition, state.level)
    .filter(node => ['active', 'regressed'].includes(getNodeState(state, node.id).status));
  if (unresolved.length > 0) {
    stopUnchangedFeatures(state);
    unresolved = nodesAt(state.definition, state.level)
      .filter(node => ['active', 'regressed'].includes(getNodeState(state, node.id).status));
    if (unresolved.length === 0) {
      openLevel(state, state.level + 1);
      return state;
    }
    return state;
  }
  openLevel(state, state.level + 1);
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
    || getNodeState(inputState, node.id).status !== 'exhausted'
    || getNodeState(inputState, node.id).exhaustedAtLevel !== grant.level)) {
    throw new Error(`grant level ${grant.level} includes a feature that is not an exhausted target`);
  }
  for (const node of eligible) {
    if (!node) continue;
    if (!Number.isSafeInteger(getNodeState(inputState, node.id).strikes.budget + grant.strikes)) {
      throw new Error(`grant for feature ${node.id} exceeds the safe strike limit`);
    }
  }
  const state = structuredClone(inputState) as DependencyState;
  state.level = grant.level;
  state.phase = 'active';
  state.terminalOutcome = null;
  for (const node of eligible) {
    if (!node) continue;
    const nodeState = getNodeState(state, node.id);
    nodeState.strikes.granted += grant.strikes;
    nodeState.strikes.budget += grant.strikes;
    nodeState.status = node.level < grant.level ? 'regressed' : 'active';
    nodeState.exhaustedAtLevel = null;
    nodeState.exhaustionReason = null;
    nodeState.unchangedFailure = { fingerprint: null, count: 0 };
  }
  const affected = new Set(grant.nodeIds);
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
    !grant.nodeIds.includes(node.id) && node.level >= grant.level && affected.has(node.id))) {
    const nodeState = getNodeState(state, node.id);
    nodeState.checks = Object.fromEntries(node.gradingChecks.map(check => [check.id, null]));
    if (nodeState.strikes.used >= nodeState.strikes.budget) {
      nodeState.status = 'exhausted';
      nodeState.exhaustedAtLevel = node.level;
      nodeState.exhaustionReason = 'strikes-exhausted';
    } else {
      nodeState.status = node.level === grant.level ? 'active' : 'locked';
      nodeState.exhaustedAtLevel = null;
      nodeState.exhaustionReason = null;
    }
  }
  state.grants.push(grant);
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
  assertState(state);
  return state;
}

function assertReplayConsistent(state: DependencyState): CompiledDependencyDefinition {
  assertState(state);
  const replayed = replayDependencyMode(state.definition, state.events);
  if (!isDeepStrictEqual(replayed, state)) {
    throw new Error('dependency mode snapshot contradicts its event history');
  }
  return state.definition;
}

export function resumeDependencyMode(snapshot: ProgressionState): DependencyState {
  const state = asDependencyState(snapshot);
  assertReplayConsistent(state);
  return replayDependencyMode(state.definition, state.events);
}

export function recordDependencyResult(inputState: ProgressionState,
  inputResult: unknown): DependencyState {
  const state = asDependencyState(inputState);
  assertReplayConsistent(state);
  const event: DependencyEvent = { sequence: state.events.length + 1,
    type: 'attempt-recorded', result: structuredClone(inputResult) as DependencyResult };
  return replayDependencyMode(state.definition, [...state.events, event]);
}

export function grantDependencyStrikes(inputState: ProgressionState,
  inputGrant: unknown): DependencyState {
  const state = asDependencyState(inputState);
  assertReplayConsistent(state);
  const event: DependencyEvent = { sequence: state.events.length + 1,
    type: 'strikes-granted', grant: structuredClone(inputGrant) as DependencyStrikeGrant };
  return replayDependencyMode(state.definition, [...state.events, event]);
}

export function nextDependencyAction(inputState: ProgressionState): ProgressionAction {
  const state = asDependencyState(inputState);
  assertReplayConsistent(state);
  if (state.phase === 'terminal') {
    if (!state.terminalOutcome) throw new Error('terminal dependency state is missing its outcome');
    return { type: 'terminal', outcome: { ...state.terminalOutcome } };
  }
  const prompt = selectedPromptWork(state);
  const grading = selectedGradingWork(state);
  const hasConclusiveAttempt = state.attempts.some(attempt =>
    attempt.level === state.level && attempt.outcome === 'conclusive');
  const featureIds = prompt.nodeIds.filter(nodeId => {
    const checks = Object.values(getNodeState(state, nodeId).checks);
    return checks.every(value => value === null) || checks.some(value => value === 'fail');
  });
  const nodes = featureIds.map(nodeId => {
    const counter = getNodeState(state, nodeId).strikes;
    return { nodeId, ...counter, remaining: counter.budget - counter.used };
  });
  const maxRemaining = Math.max(0, ...nodes.map(node => node.remaining));
  return {
    type: hasConclusiveAttempt ? 'repair' : 'build',
    level: state.level,
    strikes: { scope: 'feature', maxRemaining, nodes },
    prompt,
    grading,
  };
}

export function activeDependencyNodes(inputState: ProgressionState): string[] {
  const state = asDependencyState(inputState);
  assertReplayConsistent(state);
  return currentPromptNodeIds(state);
}

function nodePoints(definition: CompiledDependencyDefinition,
  state: DependencyState, nodeId: string): PointTotals {
  const node = definition.nodes.find(candidate => candidate.id === nodeId);
  if (!node) throw new Error(`unknown dependency node ${nodeId}`);
  const passedPoints = node.gradingChecks.reduce((total, check) =>
    total + (getNodeState(state, nodeId).checks[check.id] === 'pass' ? check.points : 0), 0);
  const failedPoints = node.gradingChecks.reduce((total, check) =>
    total + (getNodeState(state, nodeId).checks[check.id] === 'fail' ? check.points : 0), 0);
  const gradedPoints = passedPoints + failedPoints;
  const availablePoints = node.gradingChecks.reduce((total, check) => total + check.points, 0);
  return { passedPoints, failedPoints, gradedPoints,
    ungradedPoints: availablePoints - gradedPoints, availablePoints };
}

const percentage = ({ passedPoints, availablePoints }: PointTotals): number =>
  (passedPoints / availablePoints) * 100;

export function scoreDependencyMode(inputState: ProgressionState): DependencyScore {
  const state = asDependencyState(inputState);
  const definition = assertReplayConsistent(state);
  const questlines = definition.questlines.map(questline => {
    const points = questline.nodes.reduce((total, nodeId) => {
      const node = nodePoints(definition, state, nodeId);
      total.passedPoints += node.passedPoints;
      total.failedPoints += node.failedPoints;
      total.gradedPoints += node.gradedPoints;
      total.ungradedPoints += node.ungradedPoints;
      total.availablePoints += node.availablePoints;
      return total;
    }, { passedPoints: 0, failedPoints: 0, gradedPoints: 0, ungradedPoints: 0,
      availablePoints: 0 });
    const final = state.phase === 'terminal';
    return { id: questline.id, title: questline.title, ...points,
      percentage: final ? percentage(points) : null,
      provisionalPercentage: null };
  });
  const uniqueChecks = definition.nodes.reduce((total, node) => {
    const points = nodePoints(definition, state, node.id);
    total.passedPoints += points.passedPoints;
    total.failedPoints += points.failedPoints;
    total.gradedPoints += points.gradedPoints;
    total.ungradedPoints += points.ungradedPoints;
    total.availablePoints += points.availablePoints;
    return total;
  }, { passedPoints: 0, failedPoints: 0, gradedPoints: 0, ungradedPoints: 0,
    availablePoints: 0 });
  const final = state.phase === 'terminal';
  const inconclusiveAttempts = state.attempts.filter(attempt => attempt.outcome === 'inconclusive').length;
  const inconclusiveByCategory = Object.fromEntries([...INCONCLUSIVE_CATEGORIES]
    .map(category => [category, state.attempts.filter(attempt => attempt.category === category).length])) as
    Record<InconclusiveCategory, number>;
  return {
    status: final ? 'final' : 'provisional',
    terminalOutcome: final && state.terminalOutcome ? { ...state.terminalOutcome } : null,
    attempts: { total: state.attempts.length, inconclusive: inconclusiveAttempts,
      inconclusiveByCategory,
      conclusive: state.attempts.length - inconclusiveAttempts },
    questlines,
    averagePercentage: final
      ? questlines.reduce((total, questline) => total + (questline.percentage ?? 0), 0)
        / questlines.length
      : null,
    uniqueChecks: { ...uniqueChecks,
      percentage: final ? percentage(uniqueChecks) : null,
      provisionalPercentage: null },
  };
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
  resume: resumeDependencyMode,
  nextAction: nextDependencyAction,
  score: scoreDependencyMode,
});
