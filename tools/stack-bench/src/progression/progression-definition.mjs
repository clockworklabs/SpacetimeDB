import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

import { compilePackDefinition } from '../composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../composition/definition-compiler.mjs';
import { canonicalDefinitionJson, canonicalizeDefinition }
  from '../composition/definition-plan.mjs';
import { sha256 } from '../evidence/provenance.mjs';
import { compileDependencyMode, compileFeatureCatalog, DEPENDENCY_MODE_POLICY,
  DEFAULT_UNCHANGED_FAILURE_LIMIT, DEPENDENCY_MODE_SCHEMA_VERSION,
  FEATURE_CATALOG_SCHEMA_VERSION } from './dependency-mode.mjs';
import { progressionEngine } from './progression-engine.mjs';

export const PROGRESSION_DEFINITION_SCHEMA_VERSION = 2;

const ID = /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/;
const EXACT_REF = /^[a-z0-9]+(?:[._-][a-z0-9]+)*@(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-[0-9A-Za-z.-]+)?$/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at, message) => { throw new Error(`invalid progression definition at ${at}: ${message}`); };
const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

function strictObject(value, at, fields) {
  if (!object(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'unknown field');
}

function nonEmpty(value, at) {
  if (typeof value !== 'string' || value.trim().length === 0) fail(at, 'must be a non-empty string');
  return value;
}

function identifier(value, at) {
  nonEmpty(value, at);
  if (!ID.test(value)) fail(at, 'must be a lowercase identifier');
  return value;
}

function uniqueStrings(value, at, pattern = null, { nonEmpty: required = true } = {}) {
  if (!Array.isArray(value) || (required && value.length === 0)) {
    fail(at, `must be ${required ? 'a non-empty' : 'an'} array`);
  }
  const seen = new Set();
  return value.map((item, index) => {
    nonEmpty(item, `${at}[${index}]`);
    if (pattern && !pattern.test(item)) fail(`${at}[${index}]`, 'has an invalid reference');
    if (seen.has(item)) fail(`${at}[${index}]`, `duplicates ${JSON.stringify(item)}`);
    seen.add(item);
    return item;
  });
}

function packCatalog(trackRoot) {
  const packRoot = join(trackRoot, 'composition', 'packs');
  const packs = new Map();
  for (const name of readdirSync(packRoot).filter(item => item.endsWith('.json'))) {
    const pack = compilePackDefinition(readJson(join(packRoot, name)), { source: name });
    const reference = `${pack.id}@${pack.version}`;
    if (packs.has(reference)) fail('packs', `duplicate ${reference}`);
    packs.set(reference, pack);
  }
  return packs;
}

function groupChecks(pack, groupId, trackRoot, sourceCache) {
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
  if (!feature) fail(`${pack.id}.${groupId}`, `missing scenario feature ${group.feature}`);
  const criteria = group.criteria === undefined
    ? feature.criteria
    : group.criteria.map(id => {
      const criterion = feature.criteria.find(item => item.id === id);
      if (!criterion) fail(`${pack.id}.${groupId}`, `missing criterion ${id}`);
      return criterion;
    });
  return criteria.filter(criterion => criterion.points > 0).map(criterion => ({
    id: `${pack.stableId ?? pack.id}.${group.stableId ?? group.id}.${criterion.id}`,
    points: criterion.points,
  }));
}

export function compileProgressionDefinition(input, { trackRoot, source = '<progression>' } = {}) {
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
  const packs = packCatalog(resolve(trackRoot));
  const sourceCache = new Map();
  const groupOwners = new Map();
  const nodes = definition.nodes.map((node, index) => {
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
      if (!pack) fail(`${at}.featureRefs`, `missing pack ${reference}`);
      if (pack.moduleType !== 'feature') fail(`${at}.featureRefs`, `${reference} is not a feature pack`);
    }
    const promptModules = uniqueStrings([
      ...featureRefs,
      ...(node.promptModules ?? []),
    ], `${at}.promptModules`, EXACT_REF).sort();
    for (const reference of promptModules) {
      const pack = packs.get(reference);
      if (!pack) fail(`${at}.promptModules`, `missing pack ${reference}`);
      if (!['feature', 'specification'].includes(pack.moduleType)) {
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
      if (!pack) fail(`${at}.gradingGroups[${groupIndex}]`, `missing pack ${packRef}`);
      if (groupOwners.has(reference)) {
        fail(`${at}.gradingGroups[${groupIndex}]`, `is already owned by ${groupOwners.get(reference)}`);
      }
      const checks = groupChecks(pack, groupId, resolve(trackRoot), sourceCache);
      if (!checks) fail(`${at}.gradingGroups[${groupIndex}]`, `missing group ${groupId}`);
      if (checks.length === 0) fail(`${at}.gradingGroups[${groupIndex}]`, 'has no scored criteria');
      groupOwners.set(reference, node.id);
      return checks;
    });
    const selectedGroups = new Set(gradingGroups);
    for (const featureRef of featureRefs) {
      const pack = packs.get(featureRef);
      for (const group of pack.checks.filter(check => check.role === 'feature')) {
        const checks = groupChecks(pack, group.id, resolve(trackRoot), sourceCache);
        if (checks.length > 0 && !selectedGroups.has(`${featureRef}#${group.id}`)) {
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
    id: definition.id,
    version: definition.version,
    state: definition.state,
    title: definition.title,
    nodes,
    questlines: structuredClone(definition.questlines),
  }, { source });
  const byId = new Map(compiled.nodes.map(node => [node.id, node]));
  const ancestorIds = nodeId => {
    const found = new Set();
    const visit = id => {
      for (const dependency of byId.get(id).dependencies) {
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
    const promptPacks = node.promptModules.map(reference => packs.get(reference));
    if (!promptPacks.some(pack => pack.task.requirements.some(fragment =>
      fragment.modes.includes(mode)))) {
      fail(`${source}.nodes.${node.id}.promptModules`,
        `compose no ${mode} requirements at calculated level ${node.level}`);
    }
    if (!promptPacks.some(pack => pack.task.contracts.some(fragment =>
      fragment.modes.includes(mode)))) {
      fail(`${source}.nodes.${node.id}.promptModules`,
        `compose no ${mode} testing interface at calculated level ${node.level}`);
    }
    const allowedFeatures = new Set([node.id, ...ancestorIds(node.id)]
      .flatMap(id => byId.get(id).featureRefs));
    for (const featureRef of node.featureRefs) {
      for (const requiredRef of packs.get(featureRef).requiresPacks) {
        const required = packs.get(requiredRef);
        if (!required) fail(`${source}.nodes.${node.id}.featureRefs`,
          `${featureRef} requires missing pack ${requiredRef}`);
        if (required.moduleType === 'feature' && !allowedFeatures.has(requiredRef)) {
          fail(`${source}.nodes.${node.id}.dependencies`,
            `${featureRef} requires feature ${requiredRef} outside the node and its ancestors`);
        }
      }
    }
  }
  return compiled;
}

export function compileProgressionDefinitionFile(path, { trackRoot } = {}) {
  const absolute = resolve(path);
  return compileProgressionDefinition(readJson(absolute), {
    trackRoot: resolve(trackRoot ?? join(dirname(absolute), '..')),
    source: absolute,
  });
}

function identity(definition) {
  return canonicalizeDefinition({
    id: definition.id,
    version: definition.version,
    ...(definition.policy ? { policy: definition.policy } : {}),
    sha256: sha256(canonicalDefinitionJson(definition)),
  });
}

export function compileFeatureCatalogInput(input) {
  const definition = compileFeatureCatalog(input);
  return canonicalizeDefinition({ definition, identity: identity(definition) });
}

export function validateFeatureCatalogInput(input) {
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

function selectedCatalogDefinition(featureCatalog, selectedLevels) {
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

export function selectFeatureCatalogLevels(featureCatalog, selectedLevels) {
  return compileFeatureCatalogInput(
    selectedCatalogDefinition(featureCatalog, selectedLevels).definition,
  );
}

export function compileDependencyPolicyInput(strikes, featureCatalog,
  selectedLevels = progressionLevels(featureCatalog),
  unchangedFailureLimit = DEFAULT_UNCHANGED_FAILURE_LIMIT) {
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
    version: '2.0.0',
    levels: selected.levels,
    strikes: runtimeDefinition.strikes,
    unchangedFailureLimit: runtimeDefinition.unchangedFailureLimit,
  });
  return canonicalizeDefinition({
    definition,
    identity: { id: definition.id, version: definition.version,
      sha256: sha256(canonicalDefinitionJson(definition)) },
  });
}

export function validateDependencyPolicyInput(input, featureCatalog) {
  if (!object(input)) throw new Error('dependency policy input must be an object');
  const fields = new Set(['definition', 'identity']);
  for (const key of Object.keys(input)) {
    if (!fields.has(key)) throw new Error(`dependency policy input.${key} is unknown`);
  }
  const compiled = compileDependencyPolicyInput({
    levels: input.definition?.strikes?.levels ?? {},
  }, featureCatalog, input.definition?.levels,
  input.definition?.unchangedFailureLimit);
  if (canonicalDefinitionJson(input) !== canonicalDefinitionJson(compiled)) {
    throw new Error('dependency policy identity does not match its compiled definition');
  }
  return compiled;
}

export function dependencyRuntimeDefinition(featureCatalog, dependencyPolicy) {
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

export function compileProgressionInput(input) {
  const definition = progressionEngine.compile(input);
  return canonicalizeDefinition({ definition, identity: identity(definition) });
}

export function validateProgressionInput(input) {
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

export function progressionLevels(input) {
  const validator = input?.definition?.kind === 'feature-catalog'
    ? validateFeatureCatalogInput : validateProgressionInput;
  const { definition } = validator(input);
  return [...new Set(definition.nodes.map(node => node.level))].sort((left, right) => left - right);
}
