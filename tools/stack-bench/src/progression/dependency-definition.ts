import { isDeepStrictEqual } from 'node:util';

import type {
  CompiledProgressionDefinition,
  CompiledProgressionNode,
  CompiledProgressionQuestline,
} from './progression-definition.js';
import { isProgressionVersion, parseVersionedProgressionId } from './progression-identifiers.js';
import {
  assertDependencyObject as strictObject,
  dependencyFailure as fail,
  dependencyIdentifier as identifier,
  dependencyNonEmptyString as nonEmptyString,
  dependencyPositiveInteger as positiveInteger,
} from './dependency-validation.js';

export const DEPENDENCY_MODE_SCHEMA_VERSION = 4;
export const DEPENDENCY_MODE_POLICY = 'dependency-graph';
export const DEPENDENCY_MODE_VERSION = '3.2.0';
export const FEATURE_CATALOG_SCHEMA_VERSION = 1;
export const DEFAULT_UNCHANGED_FAILURE_LIMIT = 3;
export const DEFAULT_DEPENDENCY_REPAIR_SELECTION = 'feature';
export const DEPENDENCY_REPAIR_SELECTIONS = ['feature', 'batch'] as const;
export const DEFAULT_DEPENDENCY_STRIKE_POLICY = 'feature';
export const DEPENDENCY_STRIKE_POLICIES = ['feature', 'depth', 'banked'] as const;
export const DEFAULT_DEPENDENCY_WORK_SELECTION = 'progressive';
export const DEPENDENCY_WORK_SELECTIONS = ['progressive', 'all-at-once'] as const;

export type DependencyRepairSelection = typeof DEPENDENCY_REPAIR_SELECTIONS[number];
export type DependencyStrikePolicy = typeof DEPENDENCY_STRIKE_POLICIES[number];
export type DependencyWorkSelection = typeof DEPENDENCY_WORK_SELECTIONS[number];

export function isDependencyRepairSelection(value: unknown): value is DependencyRepairSelection {
  return DEPENDENCY_REPAIR_SELECTIONS.includes(value as DependencyRepairSelection);
}

export function isDependencyStrikePolicy(value: unknown): value is DependencyStrikePolicy {
  return DEPENDENCY_STRIKE_POLICIES.includes(value as DependencyStrikePolicy);
}

export function isDependencyWorkSelection(value: unknown): value is DependencyWorkSelection {
  return DEPENDENCY_WORK_SELECTIONS.includes(value as DependencyWorkSelection);
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
  repairSelection?: DependencyRepairSelection;
  strikePolicy?: DependencyStrikePolicy;
  workSelection?: DependencyWorkSelection;
  nodes: AuthoredNode[];
  questlines: Array<Omit<CompiledProgressionQuestline, 'nodes'> & { nodes?: string[] }>;
}

export interface CompiledDependencyDefinition extends CompiledProgressionDefinition {
  policy: typeof DEPENDENCY_MODE_POLICY;
  strikes: { levels: Record<string, number> };
  unchangedFailureLimit: number;
  repairSelection: DependencyRepairSelection;
  strikePolicy: DependencyStrikePolicy;
  workSelection: DependencyWorkSelection;
}

function progressionVersion(value: unknown, at: string): string {
  const result = nonEmptyString(value, at);
  if (!isProgressionVersion(result)) return fail(at, 'must be an exact semantic version');
  return result;
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
      if (!parseVersionedProgressionId(result)) {
        fail(`${at}[${index}]`, 'must be an exact id@semantic-version reference');
      }
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
  if (!catalogOnly) fields.push('policy', 'strikes', 'unchangedFailureLimit', 'repairSelection',
    'strikePolicy', 'workSelection');
  strictObject(definition, source, new Set(fields));
  const expectedSchema = catalogOnly ? FEATURE_CATALOG_SCHEMA_VERSION : DEPENDENCY_MODE_SCHEMA_VERSION;
  const expectedKind = catalogOnly ? 'feature-catalog' : 'progression-mode';
  if (definition.schemaVersion !== expectedSchema) {
    fail(`${source}.schemaVersion`, `must be ${expectedSchema}`);
  }
  if (definition.kind !== expectedKind) fail(`${source}.kind`, `must be ${JSON.stringify(expectedKind)}`);
  identifier(definition.id, `${source}.id`);
  progressionVersion(definition.version, `${source}.version`);
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
    definition.repairSelection ??= DEFAULT_DEPENDENCY_REPAIR_SELECTION;
    if (!isDependencyRepairSelection(definition.repairSelection)) {
      fail(`${source}.repairSelection`, 'must be "feature" or "batch"');
    }
    definition.strikePolicy ??= DEFAULT_DEPENDENCY_STRIKE_POLICY;
    if (!isDependencyStrikePolicy(definition.strikePolicy)) {
      fail(`${source}.strikePolicy`, 'must be "feature", "depth", or "banked"');
    }
    definition.workSelection ??= DEFAULT_DEPENDENCY_WORK_SELECTION;
    if (!isDependencyWorkSelection(definition.workSelection)) {
      fail(`${source}.workSelection`, 'must be "progressive" or "all-at-once"');
    }
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
      strictObject(check, checkAt, new Set(['id', 'points', 'role', 'requiresFeatures']));
      identifier(check.id, `${checkAt}.id`);
      if (localChecks.has(check.id)) fail(`${checkAt}.id`, `duplicates ${JSON.stringify(check.id)}`);
      if (checkIds.has(check.id)) fail(`${checkAt}.id`, 'is already owned by another node');
      localChecks.add(check.id);
      checkIds.add(check.id);
      positiveInteger(check.points, `${checkAt}.points`);
      if (!['feature', 'guarantee'].includes(check.role)) {
        fail(`${checkAt}.role`, 'must be "feature" or "guarantee"');
      }
      if (check.requiresFeatures !== undefined) {
        check.requiresFeatures = uniqueStrings(check.requiresFeatures,
          `${checkAt}.requiresFeatures`).sort();
      }
    });
    if (!node.gradingChecks.some(check => check.role === 'feature')) {
      fail(`${at}.gradingChecks`, 'must contain at least one feature check');
    }
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
  const featureOwners = new Map<string, string>();
  for (const node of definition.nodes) {
    for (const reference of node.featureRefs) {
      const parsed = parseVersionedProgressionId(reference);
      if (!parsed) return fail(`${source}.nodes.${node.id}.featureRefs`, `invalid reference ${reference}`);
      const priorOwner = featureOwners.get(parsed.id);
      if (priorOwner !== undefined) {
        fail(`${source}.nodes.${node.id}.featureRefs`,
          `${parsed.id} is already owned by ${priorOwner}`);
      }
      featureOwners.set(parsed.id, node.id);
    }
  }
  for (const node of definition.nodes) {
    const at = `${source}.nodes.${node.id}.dependencies`;
    for (const dependency of node.dependencies) {
      if (!nodesById.has(dependency as string)) fail(at, `unknown parent ${JSON.stringify(dependency)}`);
    }
    for (const check of node.gradingChecks) {
      for (const featureId of check.requiresFeatures ?? []) {
        if (!featureOwners.has(featureId)) {
          fail(`${source}.nodes.${node.id}.gradingChecks.${check.id}.requiresFeatures`,
            `references unknown feature ${JSON.stringify(featureId)}`);
        }
      }
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
