import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

import {
  compilePackDefinition,
  type CompiledPackDefinition,
} from '../composition/composition-compiler.mjs';
import {
  compileScenarioDefinition,
  type CompiledScenarioDefinition,
} from '../composition/definition-compiler.mjs';
import { canonicalDefinitionJson, canonicalizeDefinition }
  from '../composition/definition-plan.mjs';
import { sha256 } from '../evidence/provenance.mjs';
import { compileDependencyMode, compileFeatureCatalog, DEPENDENCY_MODE_POLICY,
  DEFAULT_UNCHANGED_FAILURE_LIMIT, DEPENDENCY_MODE_SCHEMA_VERSION,
  FEATURE_CATALOG_SCHEMA_VERSION } from './dependency-mode.js';
import { progressionEngine } from './progression-engine.js';

export interface CompiledProgressionCheck {
  id: string;
  points: number;
}

export interface CompiledProgressionNode {
  id: string;
  title: string;
  level: number;
  questline: string;
  featureRefs: string[];
  promptModules: string[];
  gradingChecks: CompiledProgressionCheck[];
  dependencies: string[];
  dependencyReasons: Record<string, string>;
}

export interface CompiledProgressionQuestline {
  id: string;
  title: string;
  nodes: string[];
}

export interface CompiledProgressionDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  nodes: CompiledProgressionNode[];
  questlines: CompiledProgressionQuestline[];
  policy?: string;
  strikes?: { levels: Record<string, number> };
  unchangedFailureLimit?: number;
}

export interface DefinitionIdentity {
  id: string;
  version: string;
  sha256: string;
  policy?: string;
}

export interface ProgressionInput<TDefinition = CompiledProgressionDefinition> {
  definition: TDefinition;
  identity: DefinitionIdentity;
}

export interface CompiledDependencyPolicyDefinition extends Record<string, unknown> {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  levels: number[];
  strikes: { levels: Record<string, number> };
  unchangedFailureLimit: number;
}

interface AuthoredDependency {
  id: string;
  reason: string;
}

interface AuthoredProgressionNode {
  id: string;
  title: string;
  questline: string;
  featureRefs: string[];
  promptModules?: string[];
  gradingGroups: string[];
  dependencies: AuthoredDependency[];
}

interface AuthoredProgressionDefinition {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: string;
  title: string;
  nodes: AuthoredProgressionNode[];
  questlines: unknown[];
}

export const PROGRESSION_DEFINITION_SCHEMA_VERSION = 2;

const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const EXACT_REF = /^[a-z0-9]+(?:[._-][a-z0-9]+)*@(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const object = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at: string, message: string): never => {
  throw new Error(`invalid progression definition at ${at}: ${message}`);
};
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8')) as unknown;

function strictObject(value: unknown, at: string, fields: ReadonlySet<string>): asserts value is Record<string, unknown> {
  if (!object(value)) return fail(at, 'must be an object');
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'unknown field');
}

function nonEmpty(value: unknown, at: string): string {
  if (typeof value !== 'string' || value.trim().length === 0) {
    return fail(at, 'must be a non-empty string');
  }
  return value;
}

function identifier(value: unknown, at: string): string {
  const result = nonEmpty(value, at);
  if (!ID.test(result)) return fail(at, 'must be a lowercase identifier');
  return result;
}

function uniqueStrings(value: unknown, at: string, pattern: RegExp | null = null,
  { nonEmpty: required = true }: { nonEmpty?: boolean } = {}): string[] {
  if (!Array.isArray(value)) {
    return fail(at, `must be ${required ? 'a non-empty' : 'an'} array`);
  }
  if (required && value.length === 0) {
    return fail(at, 'must be a non-empty array');
  }
  const seen = new Set<string>();
  return value.map((item, index) => {
    const result = nonEmpty(item, `${at}[${index}]`);
    if (pattern && !pattern.test(result)) fail(`${at}[${index}]`, 'has an invalid reference');
    if (seen.has(result)) fail(`${at}[${index}]`, `duplicates ${JSON.stringify(result)}`);
    seen.add(result);
    return result;
  });
}

function packCatalog(trackRoot: string): Map<string, CompiledPackDefinition> {
  const packRoot = join(trackRoot, 'composition', 'packs');
  const packs = new Map<string, CompiledPackDefinition>();
  for (const name of readdirSync(packRoot).filter(item => item.endsWith('.json'))) {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    const reference = `${pack.id}@${pack.version}`;
    if (packs.has(reference)) fail('packs', `duplicate ${reference}`);
    packs.set(reference, pack);
  }
  return packs;
}

function groupChecks(pack: CompiledPackDefinition, groupId: string, trackRoot: string,
  sourceCache: Map<string, CompiledScenarioDefinition>): CompiledProgressionCheck[] | null {
  const group = pack.checks.find(check => check.id === groupId);
  if (!group) return null;
  let scenario = sourceCache.get(group.source);
  if (!scenario) {
    scenario = compileScenarioDefinition(readJson(join(trackRoot, group.source)), {
      source: group.source,
    });
    sourceCache.set(group.source, scenario);
  }
  const feature = scenario.features.find(item => item.id === group.feature);
  if (!feature) return fail(`${pack.id}.${groupId}`, `missing scenario feature ${group.feature}`);
  const criteria = group.criteria === undefined
    ? feature.criteria
    : group.criteria.map(id => {
      const criterion = feature.criteria.find(item => item.id === id);
      if (!criterion) return fail(`${pack.id}.${groupId}`, `missing criterion ${id}`);
      return criterion;
    });
  return criteria.filter(criterion => criterion.points > 0).map(criterion => ({
    id: `${pack.stableId ?? pack.id}.${group.stableId ?? group.id}.${criterion.id}`,
    points: criterion.points,
  }));
}

export function compileProgressionDefinition(input: unknown,
  { trackRoot, source = '<progression>' }: { trackRoot?: string; source?: string } = {}): CompiledProgressionDefinition {
  if (!trackRoot) throw new Error('progression compilation requires trackRoot');
  const definition = structuredClone(input);
  strictObject(definition, source, new Set([
    'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'nodes', 'questlines',
  ]));
  if (definition.schemaVersion !== PROGRESSION_DEFINITION_SCHEMA_VERSION) {
    fail(`${source}.schemaVersion`, `must be ${PROGRESSION_DEFINITION_SCHEMA_VERSION}`);
  }
  if (definition.kind !== 'progression-definition') {
    fail(`${source}.kind`, 'must be "progression-definition"');
  }
  if (!Array.isArray(definition.nodes) || definition.nodes.length === 0) {
    fail(`${source}.nodes`, 'must be a non-empty array');
  }
  const authored = definition as unknown as AuthoredProgressionDefinition;
  const packs = packCatalog(resolve(trackRoot));
  const sourceCache = new Map<string, CompiledScenarioDefinition>();
  const groupOwners = new Map<string, string>();
  const nodes = authored.nodes.map((node, index) => {
    const at = `${source}.nodes[${index}]`;
    strictObject(node, at, new Set([
      'id', 'title', 'questline', 'featureRefs', 'promptModules', 'gradingGroups', 'dependencies',
    ]));
    identifier(node.id, `${at}.id`);
    nonEmpty(node.title, `${at}.title`);
    identifier(node.questline, `${at}.questline`);
    const featureRefs = uniqueStrings(node.featureRefs, `${at}.featureRefs`, EXACT_REF).sort();
    for (const reference of featureRefs) {
      const pack = packs.get(reference);
      if (!pack) return fail(`${at}.featureRefs`, `missing pack ${reference}`);
      if (pack.moduleType !== 'feature') fail(`${at}.featureRefs`, `${reference} is not a feature pack`);
    }
    const promptModules = uniqueStrings([
      ...featureRefs,
      ...(node.promptModules ?? []),
    ], `${at}.promptModules`, EXACT_REF).sort();
    for (const reference of promptModules) {
      const pack = packs.get(reference);
      if (!pack) return fail(`${at}.promptModules`, `missing pack ${reference}`);
      if (!['feature', 'specification'].includes(pack.moduleType ?? '')) {
        fail(`${at}.promptModules`, `${reference} cannot be used in a prompt`);
      }
      if (pack.moduleType === 'feature' && !featureRefs.includes(reference)) {
        fail(`${at}.promptModules`, `${reference} must also appear in featureRefs`);
      }
    }
    const gradingGroups = uniqueStrings(node.gradingGroups, `${at}.gradingGroups`).sort();
    const gradingChecks = gradingGroups.flatMap((reference, groupIndex) => {
      const split = reference.lastIndexOf('#');
      if (split < 1) fail(`${at}.gradingGroups[${groupIndex}]`, 'must be pack@version#group');
      const packRef = reference.slice(0, split);
      const groupId = reference.slice(split + 1);
      if (!EXACT_REF.test(packRef)) fail(`${at}.gradingGroups[${groupIndex}]`, 'has an invalid pack reference');
      identifier(groupId, `${at}.gradingGroups[${groupIndex}] group`);
      const pack = packs.get(packRef);
      if (!pack) return fail(`${at}.gradingGroups[${groupIndex}]`, `missing pack ${packRef}`);
      if (groupOwners.has(reference)) {
        fail(`${at}.gradingGroups[${groupIndex}]`, `is already owned by ${groupOwners.get(reference)}`);
      }
      const checks = groupChecks(pack, groupId, resolve(trackRoot), sourceCache);
      if (!checks) return fail(`${at}.gradingGroups[${groupIndex}]`, `missing group ${groupId}`);
      if (checks.length === 0) fail(`${at}.gradingGroups[${groupIndex}]`, 'has no scored criteria');
      groupOwners.set(reference, node.id);
      return checks;
    });
    const selectedGroups = new Set(gradingGroups);
    for (const featureRef of featureRefs) {
      const pack = packs.get(featureRef);
      if (!pack) return fail(`${at}.featureRefs`, `missing pack ${featureRef}`);
      for (const group of pack.checks.filter(check => check.role === 'feature')) {
        const checks = groupChecks(pack, group.id, resolve(trackRoot), sourceCache);
        if (checks !== null && checks.length > 0 && !selectedGroups.has(`${featureRef}#${group.id}`)) {
          fail(`${at}.gradingGroups`, `must own feature group ${featureRef}#${group.id}`);
        }
      }
    }
    if (!Array.isArray(node.dependencies)) fail(`${at}.dependencies`, 'must be an array');
    return {
      id: node.id,
      title: node.title,
      questline: node.questline,
      featureRefs,
      promptModules,
      gradingChecks,
      dependencies: structuredClone(node.dependencies),
    };
  });
  const compiled = compileFeatureCatalog({
    schemaVersion: FEATURE_CATALOG_SCHEMA_VERSION,
    kind: 'feature-catalog',
    id: authored.id,
    version: authored.version,
    state: authored.state,
    title: authored.title,
    nodes,
    questlines: structuredClone(authored.questlines),
  }, { source });
  const byId = new Map(compiled.nodes.map(node => [node.id, node]));
  const ancestorIds = (nodeId: string): Set<string> => {
    const found = new Set<string>();
    const visit = (id: string): void => {
      const current = byId.get(id);
      if (!current) return fail(`${source}.nodes.${id}`, 'is missing from the compiled catalog');
      for (const dependency of current.dependencies) {
        if (found.has(dependency)) continue;
        found.add(dependency);
        visit(dependency);
      }
    };
    visit(nodeId);
    return found;
  };
  for (const node of compiled.nodes) {
    const mode = node.level === 1 ? 'fresh' : 'upgrade';
    const promptPacks = node.promptModules.map(reference => {
      const pack = packs.get(reference);
      if (!pack) return fail(`${source}.nodes.${node.id}.promptModules`, `missing pack ${reference}`);
      return pack;
    });
    if (!promptPacks.some(pack => pack.task.requirements.some(fragment =>
      (fragment.modes ?? []).includes(mode)))) {
      fail(`${source}.nodes.${node.id}.promptModules`,
        `compose no ${mode} requirements at calculated level ${node.level}`);
    }
    if (!promptPacks.some(pack => pack.task.contracts.some(fragment =>
      (fragment.modes ?? []).includes(mode)))) {
      fail(`${source}.nodes.${node.id}.promptModules`,
        `compose no ${mode} testing interface at calculated level ${node.level}`);
    }
    const allowedFeatures = new Set([node.id, ...ancestorIds(node.id)]
      .flatMap(id => byId.get(id)?.featureRefs ?? []));
    for (const featureRef of node.featureRefs) {
      const featurePack = packs.get(featureRef);
      if (!featurePack) return fail(`${source}.nodes.${node.id}.featureRefs`, `missing pack ${featureRef}`);
      for (const requiredRef of featurePack.requiresPacks) {
        const required = packs.get(requiredRef);
        if (!required) return fail(`${source}.nodes.${node.id}.featureRefs`,
          `${featureRef} requires missing pack ${requiredRef}`);
        if (required.moduleType === 'feature' && !allowedFeatures.has(requiredRef)) {
          fail(`${source}.nodes.${node.id}.dependencies`,
            `${featureRef} requires feature ${requiredRef} outside the node and its ancestors`);
        }
      }
    }
  }
  return compiled as CompiledProgressionDefinition;
}

export function compileProgressionDefinitionFile(path: string,
  { trackRoot }: { trackRoot?: string } = {}): CompiledProgressionDefinition {
  const absolute = resolve(path);
  return compileProgressionDefinition(readJson(absolute), {
    trackRoot: resolve(trackRoot ?? join(dirname(absolute), '..')),
    source: absolute,
  });
}

function identity(definition: CompiledProgressionDefinition): DefinitionIdentity {
  return canonicalizeDefinition({
    id: definition.id,
    version: definition.version,
    ...(definition.policy ? { policy: definition.policy } : {}),
    sha256: sha256(canonicalDefinitionJson(definition)),
  }) as unknown as DefinitionIdentity;
}

export function compileFeatureCatalogInput(input: unknown): ProgressionInput {
  const definition = compileFeatureCatalog(input);
  return canonicalizeDefinition({ definition, identity: identity(definition) }) as unknown as ProgressionInput;
}

export function validateFeatureCatalogInput(input: unknown): ProgressionInput {
  if (!object(input)) throw new Error('feature catalog input must be an object');
  const fields = new Set(['definition', 'identity']);
  for (const key of Object.keys(input)) {
    if (!fields.has(key)) throw new Error(`feature catalog input.${key} is unknown`);
  }
  const compiled = compileFeatureCatalogInput(input.definition);
  if (canonicalDefinitionJson(input) !== canonicalDefinitionJson(compiled)) {
    throw new Error('feature catalog identity does not match its compiled definition');
  }
  return compiled;
}

function selectedCatalogDefinition(featureCatalog: unknown, selectedLevels: unknown): {
  catalog: ProgressionInput;
  levels: number[];
  definition: CompiledProgressionDefinition;
} {
  const catalog = validateFeatureCatalogInput(featureCatalog);
  const availableLevels = progressionLevels(catalog);
  if (!Array.isArray(selectedLevels) || selectedLevels.length === 0
    || selectedLevels.some(level => !Number.isSafeInteger(level) || level < 1)
    || new Set(selectedLevels).size !== selectedLevels.length) {
    throw new Error('dependency policy levels must be distinct positive integers');
  }
  const levels = [...selectedLevels].sort((left, right) => left - right);
  if (canonicalDefinitionJson(levels)
    !== canonicalDefinitionJson(availableLevels.slice(0, levels.length))) {
    throw new Error('dependency policy levels must be a prefix of the feature catalog');
  }
  const selected = new Set(levels);
  const nodes = catalog.definition.nodes.filter(node => selected.has(node.level));
  const nodeIds = new Set(nodes.map(node => node.id));
  const questlines = catalog.definition.questlines
    .map(questline => ({ ...questline,
      nodes: questline.nodes.filter(nodeId => nodeIds.has(nodeId)) }))
    .filter(questline => questline.nodes.length > 0);
  return { catalog, levels, definition: { ...catalog.definition, nodes, questlines } };
}

export function selectFeatureCatalogLevels(featureCatalog: unknown,
  selectedLevels: unknown): ProgressionInput {
  return compileFeatureCatalogInput(
    selectedCatalogDefinition(featureCatalog, selectedLevels).definition,
  );
}

export function compileDependencyPolicyInput(strikes: unknown, featureCatalog: unknown,
  selectedLevels: number[] = progressionLevels(featureCatalog),
  unchangedFailureLimit = DEFAULT_UNCHANGED_FAILURE_LIMIT): ProgressionInput<CompiledDependencyPolicyDefinition> {
  const catalog = validateFeatureCatalogInput(featureCatalog);
  const selected = selectedCatalogDefinition(catalog, selectedLevels);
  const runtimeDefinition = compileDependencyMode({
    ...selected.definition,
    schemaVersion: DEPENDENCY_MODE_SCHEMA_VERSION,
    kind: 'progression-mode',
    policy: DEPENDENCY_MODE_POLICY,
    strikes,
    unchangedFailureLimit,
  });
  const definition = canonicalizeDefinition({
    schemaVersion: 1,
    kind: 'dependency-policy',
    id: DEPENDENCY_MODE_POLICY,
    version: '2.1.0',
    levels: selected.levels,
    strikes: runtimeDefinition.strikes,
    unchangedFailureLimit: runtimeDefinition.unchangedFailureLimit,
  }) as CompiledDependencyPolicyDefinition;
  return canonicalizeDefinition({
    definition,
    identity: { id: definition.id, version: definition.version,
      sha256: sha256(canonicalDefinitionJson(definition)) },
  }) as unknown as ProgressionInput<CompiledDependencyPolicyDefinition>;
}

export function validateDependencyPolicyInput(input: unknown,
  featureCatalog: unknown): ProgressionInput<CompiledDependencyPolicyDefinition> {
  if (!object(input)) throw new Error('dependency policy input must be an object');
  const fields = new Set(['definition', 'identity']);
  for (const key of Object.keys(input)) {
    if (!fields.has(key)) throw new Error(`dependency policy input.${key} is unknown`);
  }
  if (!object(input.definition) || !object(input.definition.strikes)) {
    throw new Error('dependency policy input definition is incomplete');
  }
  if (!Array.isArray(input.definition.levels)
    || input.definition.levels.some(level => !Number.isSafeInteger(level))) {
    throw new Error('dependency policy input levels are invalid');
  }
  const levels = input.definition.levels as number[];
  const compiled = compileDependencyPolicyInput({
    levels: input.definition.strikes.levels ?? {},
  }, featureCatalog, levels,
  input.definition.unchangedFailureLimit as number | undefined);
  if (canonicalDefinitionJson(input) !== canonicalDefinitionJson(compiled)) {
    throw new Error('dependency policy identity does not match its compiled definition');
  }
  return compiled;
}

export function dependencyRuntimeDefinition(featureCatalog: unknown,
  dependencyPolicy: unknown): CompiledProgressionDefinition {
  const catalog = validateFeatureCatalogInput(featureCatalog);
  const policy = validateDependencyPolicyInput(dependencyPolicy, catalog);
  const selected = selectedCatalogDefinition(catalog, policy.definition.levels);
  return compileDependencyMode({
    ...selected.definition,
    schemaVersion: DEPENDENCY_MODE_SCHEMA_VERSION,
    kind: 'progression-mode',
    policy: policy.definition.id,
    strikes: { levels: policy.definition.strikes.levels },
    unchangedFailureLimit: policy.definition.unchangedFailureLimit,
  });
}

export function compileProgressionInput(input: unknown): ProgressionInput {
  const definition = progressionEngine.compile(input);
  return canonicalizeDefinition({ definition, identity: identity(definition) }) as unknown as ProgressionInput;
}

export function validateProgressionInput(input: unknown): ProgressionInput {
  if (!object(input)) throw new Error('progression input must be an object');
  const fields = new Set(['definition', 'identity']);
  for (const key of Object.keys(input)) {
    if (!fields.has(key)) throw new Error(`progression input.${key} is unknown`);
  }
  const compiled = compileProgressionInput(input.definition);
  if (canonicalDefinitionJson(input) !== canonicalDefinitionJson(compiled)) {
    throw new Error('progression input identity does not match its compiled definition');
  }
  return compiled;
}

export function progressionLevels(input: unknown): number[] {
  const catalog = object(input) && object(input.definition)
    && input.definition.kind === 'feature-catalog';
  const validator = catalog
    ? validateFeatureCatalogInput : validateProgressionInput;
  const { definition } = validator(input);
  return [...new Set(definition.nodes.map(node => node.level))].sort((left, right) => left - right);
}
