import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

import { compilePackDefinition } from '../composition/composition-compiler.mjs';
import { compileScenarioDefinition } from '../composition/definition-compiler.mjs';
import { compileDependencyMode } from './dependency-mode.mjs';

export const PROGRESSION_DEFINITION_SCHEMA_VERSION = 1;

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

function uniqueStrings(value, at, pattern = null) {
  if (!Array.isArray(value) || value.length === 0) fail(at, 'must be a non-empty array');
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
    'schemaVersion', 'kind', 'id', 'version', 'state', 'title', 'policy', 'strikes', 'nodes',
    'questlines',
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
      'id', 'title', 'questline', 'featureRefs', 'gradingGroups', 'dependencies',
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
      promptModules: featureRefs,
      gradingChecks,
      dependencies: structuredClone(node.dependencies),
    };
  });
  return compileDependencyMode({
    schemaVersion: 2,
    kind: 'progression-mode',
    id: definition.id,
    version: definition.version,
    state: definition.state,
    title: definition.title,
    policy: definition.policy,
    strikes: definition.strikes,
    nodes,
    questlines: structuredClone(definition.questlines),
  }, { source });
}

export function compileProgressionDefinitionFile(path, { trackRoot } = {}) {
  const absolute = resolve(path);
  return compileProgressionDefinition(readJson(absolute), {
    trackRoot: resolve(trackRoot ?? join(dirname(absolute), '..')),
    source: absolute,
  });
}
