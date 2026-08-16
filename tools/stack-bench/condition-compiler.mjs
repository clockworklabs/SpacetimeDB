import { readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';

import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.mjs';
import { readAgentSkillDocuments } from './agent-materials.mjs';
import { sha256 } from './provenance.mjs';

const ROOT = dirname(fileURLToPath(import.meta.url));
const CATALOG = resolve(ROOT, 'conditions', 'catalog.json');
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const HASH = /^[a-f0-9]{64}$/;
const REF = /^([a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*)@(\d+\.\d+\.\d+)$/;
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const fail = (at, message) => { throw new Error(`invalid study condition at ${at}: ${message}`); };

function strict(value, at, fields) {
  if (!object(value)) fail(at, 'must be an object');
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'is unknown');
  for (const key of fields) if (!Object.hasOwn(value, key)) fail(`${at}.${key}`, 'is required');
}

function contained(root, path, at) {
  const absoluteRoot = realpathSync(root);
  const absolute = realpathSync(resolve(absoluteRoot, path));
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) fail(at, 'escapes the condition root');
  return absolute;
}

function json(path, at) {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { fail(at, `cannot be read: ${error.message}`); }
}

function identity(profile, resolved) {
  return { id: profile.id, version: profile.version,
    sha256: sha256(canonicalDefinitionJson({ profile, resolved })), state: profile.state };
}

function validateIdentityFields(value, at, kind) {
  if (value.schemaVersion !== 1) fail(`${at}.schemaVersion`, 'must be 1');
  if (value.kind !== kind) fail(`${at}.kind`, `must be ${kind}`);
  if (!ID.test(value.id)) fail(`${at}.id`, 'is invalid');
  if (!VERSION.test(value.version)) fail(`${at}.version`, 'must be a semantic version');
  if (!['draft', 'qualified'].includes(value.state)) fail(`${at}.state`, 'must be draft or qualified');
}

function loadCatalog(path = CATALOG) {
  const value = json(path, 'catalog');
  strict(value, 'catalog', new Set(['schemaVersion', 'kind', 'guidanceProfiles',
    'probeProfiles', 'repairPolicies']));
  if (value.schemaVersion !== 1 || value.kind !== 'study-condition-catalog') {
    fail('catalog', 'has an unsupported schema or kind');
  }
  for (const field of ['guidanceProfiles', 'probeProfiles', 'repairPolicies']) {
    if (!object(value[field])) fail(`catalog.${field}`, 'must be an object');
    for (const [key, pathValue] of Object.entries(value[field])) {
      if (!REF.test(key) || typeof pathValue !== 'string' || !pathValue) {
        fail(`catalog.${field}.${key}`, 'must map an id@version to a path');
      }
    }
  }
  return { value, root: dirname(path) };
}

function loadProfile(catalog, section, reference, kind, fields) {
  const match = REF.exec(reference ?? '');
  if (!match) fail(reference ?? '<missing>', 'must use id@version');
  const rel = catalog.value[section][reference];
  if (!rel) fail(reference, `is not in catalog.${section}`);
  const path = contained(catalog.root, rel, reference);
  const profile = json(path, reference);
  strict(profile, reference, fields);
  validateIdentityFields(profile, reference, kind);
  if (profile.id !== match[1] || profile.version !== match[2]) fail(reference, 'does not match the loaded profile identity');
  return { profile, path };
}

function resolveGuidance(catalog, reference, stacks, stackBenchRoot) {
  const fields = new Set(['schemaVersion', 'kind', 'id', 'version', 'state', 'mode', 'material',
    'documents', 'skills']);
  const { profile } = loadProfile(catalog, 'guidanceProfiles', reference,
    'backend-guidance-profile', fields);
  if (!['prescribed', 'neutral'].includes(profile.mode)) fail(`${reference}.mode`, 'must be prescribed or neutral');
  strict(profile.material, `${reference}.material`, new Set(['accessFacts', 'apiReference', 'designAdvice']));
  for (const field of ['accessFacts', 'apiReference', 'designAdvice']) {
    if (typeof profile.material[field] !== 'boolean') fail(`${reference}.material.${field}`, 'must be boolean');
  }
  if (profile.mode === 'neutral' && profile.material.designAdvice) {
    fail(`${reference}.material.designAdvice`, 'must be false for neutral guidance');
  }
  if (!object(profile.documents)) fail(`${reference}.documents`, 'must be an object');
  if (!object(profile.skills)) fail(`${reference}.skills`, 'must be an object');
  const documents = {};
  const skills = {};
  for (const stack of stacks) {
    const rel = profile.documents[stack];
    if (typeof rel !== 'string' || !rel) fail(`${reference}.documents.${stack}`, 'is required');
    const path = contained(stackBenchRoot, rel, `${reference}.documents.${stack}`);
    const bytes = readFileSync(path);
    documents[stack] = { path: relative(stackBenchRoot, path).split(sep).join('/'),
      sha256: sha256(bytes), bytes: bytes.length };
    const ids = profile.skills[stack];
    if (!Array.isArray(ids) || new Set(ids).size !== ids.length
      || ids.some(id => typeof id !== 'string' || !/^[a-z][a-z0-9-]*$/.test(id))) {
      fail(`${reference}.skills.${stack}`, 'must contain unique skill ids');
    }
    let text;
    try { text = readAgentSkillDocuments(resolve(stackBenchRoot, '..', '..'), ids); }
    catch (error) { fail(`${reference}.skills.${stack}`, `cannot be read: ${error.message}`); }
    skills[stack] = { ids: [...ids], sha256: sha256(text), bytes: Buffer.byteLength(text) };
  }
  return { ...identity(profile, { documents, skills }), mode: profile.mode,
    material: profile.material, documents, skills };
}

// Public review surface for tooling that must inspect the exact guidance
// selected by a profile without manufacturing an otherwise unrelated study
// condition. Campaign compilation uses the same resolver below.
export function resolveGuidanceProfile(reference, stacks,
  { stackBenchRoot = ROOT, catalogPath = CATALOG } = {}) {
  if (!Array.isArray(stacks) || stacks.length === 0
    || new Set(stacks).size !== stacks.length
    || stacks.some(stack => typeof stack !== 'string' || !ID.test(stack))) {
    fail('stacks', 'must contain unique stack ids');
  }
  return resolveGuidance(loadCatalog(catalogPath), reference, stacks, resolve(stackBenchRoot));
}

function resolveProbes(catalog, reference) {
  const fields = new Set(['schemaVersion', 'kind', 'id', 'version', 'state', 'firstBuildOnly',
    'scoreContribution', 'repairVisible', 'probes']);
  const { profile } = loadProfile(catalog, 'probeProfiles', reference,
    'capability-probe-profile', fields);
  if (profile.firstBuildOnly !== true) fail(`${reference}.firstBuildOnly`, 'must be true');
  if (profile.scoreContribution !== false) fail(`${reference}.scoreContribution`, 'must be false');
  if (profile.repairVisible !== false) fail(`${reference}.repairVisible`, 'must be false');
  if (!Array.isArray(profile.probes) || new Set(profile.probes).size !== profile.probes.length
    || profile.probes.some(probe => typeof probe !== 'string' || !ID.test(probe))) {
    fail(`${reference}.probes`, 'must contain unique capability ids');
  }
  if (profile.probes.length > 0) {
    fail(`${reference}.probes`, 'names probes that this engine does not execute yet');
  }
  return { ...identity(profile, profile.probes), firstBuildOnly: true, scoreContribution: false,
    repairVisible: false, probes: [...profile.probes].sort() };
}

function resolveRepair(catalog, reference) {
  const fields = new Set(['schemaVersion', 'kind', 'id', 'version', 'state',
    'requestedEvidence', 'probeEvidence']);
  const { profile } = loadProfile(catalog, 'repairPolicies', reference, 'repair-policy', fields);
  if (typeof profile.requestedEvidence !== 'boolean' || typeof profile.probeEvidence !== 'boolean') {
    fail(reference, 'repair evidence fields must be boolean');
  }
  if (!profile.requestedEvidence) {
    fail(`${reference}.requestedEvidence`, 'must be true until no-evidence repair is implemented');
  }
  if (profile.probeEvidence) fail(`${reference}.probeEvidence`, 'must be false');
  return { ...identity(profile, null), requestedEvidence: profile.requestedEvidence, probeEvidence: false };
}

export function validateConditionReference(input, at = 'condition') {
  if (!object(input)) fail(at, 'must be an object');
  const value = structuredClone(input);
  const fields = new Set(['id', 'version', 'guidanceProfile', 'probeProfile', 'repairPolicy',
    'specifications']);
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'is unknown');
  for (const key of ['id', 'version', 'guidanceProfile', 'probeProfile', 'repairPolicy']) {
    if (!Object.hasOwn(value, key)) fail(`${at}.${key}`, 'is required');
  }
  if (!ID.test(value.id)) fail(`${at}.id`, 'is invalid');
  if (!VERSION.test(value.version)) fail(`${at}.version`, 'must be a semantic version');
  for (const field of ['guidanceProfile', 'probeProfile', 'repairPolicy']) {
    if (!REF.test(value[field] ?? '')) fail(`${at}.${field}`, 'must use id@version');
  }
  if (value.specifications !== undefined) {
    strict(value.specifications, `${at}.specifications`, new Set(['levels']));
    if (!Array.isArray(value.specifications.levels) || value.specifications.levels.length === 0) {
      fail(`${at}.specifications.levels`, 'must be a non-empty array');
    }
    const seen = new Set();
    value.specifications.levels.forEach((entry, index) => {
      const levelAt = `${at}.specifications.levels[${index}]`;
      strict(entry, levelAt, new Set(['level', 'disclosed', 'probed']));
      if (!Number.isSafeInteger(entry.level) || entry.level < 1 || seen.has(entry.level)) {
        fail(`${levelAt}.level`, 'must be a unique positive integer');
      }
      seen.add(entry.level);
      for (const field of ['disclosed', 'probed']) {
        if (!Array.isArray(entry[field]) || new Set(entry[field]).size !== entry[field].length
          || entry[field].some(reference => !REF.test(reference))) {
          fail(`${levelAt}.${field}`, 'must contain unique exact id@version references');
        }
        entry[field] = [...entry[field]].sort();
      }
      const overlap = entry.disclosed.filter(reference => entry.probed.includes(reference));
      if (overlap.length) fail(levelAt, `cannot disclose and probe ${overlap.join(', ')}`);
    });
    value.specifications.levels.sort((left, right) => left.level - right.level);
  }
  return canonicalizeDefinition(value);
}

function validateRequestedScope(input) {
  strict(input, 'requested', new Set(['track', 'levels']));
  if (!ID.test(input.track)) fail('requested.track', 'is invalid');
  if (!Array.isArray(input.levels) || input.levels.length === 0) {
    fail('requested.levels', 'must be a non-empty array');
  }
  const levels = input.levels.map((entry, index) => {
    const at = `requested.levels[${index}]`;
    strict(entry, at, new Set(['level', 'recipe', 'selection', 'task']));
    if (!Number.isSafeInteger(entry.level) || entry.level < 1) fail(`${at}.level`, 'must be positive');
    strict(entry.recipe, `${at}.recipe`, new Set(['id', 'version', 'contentSha256',
      'meaningSha256', 'executionSha256', 'state']));
    if (!ID.test(entry.recipe.id) || !VERSION.test(entry.recipe.version)
      || !['draft', 'qualified'].includes(entry.recipe.state)
      || ['contentSha256', 'meaningSha256', 'executionSha256']
        .some(field => !HASH.test(entry.recipe[field]))) {
      fail(`${at}.recipe`, 'has an invalid identity');
    }
    const modular = entry.selection?.schemaVersion === 2;
    strict(entry.selection, `${at}.selection`, modular
      ? new Set(['schemaVersion', 'sha256', 'scoredPoints', 'requested', 'taskPacks',
        'features', 'specifications', 'requestedChecks', 'probeChecks'])
      : new Set(['sha256', 'completeness', 'scoredPoints', 'requested', 'taskPacks']));
    if (!HASH.test(entry.selection.sha256)
      || !Number.isSafeInteger(entry.selection.scoredPoints) || entry.selection.scoredPoints < 0
      || (!modular && !['full', 'subset'].includes(entry.selection.completeness))) {
      fail(`${at}.selection`, 'has an invalid identity');
    }
    strict(entry.selection.requested, `${at}.selection.requested`, modular
      ? new Set(['features', 'specifications', 'checks']) : new Set(['packs', 'checks']));
    if (!Array.isArray(entry.selection.taskPacks)
      || new Set(entry.selection.taskPacks).size !== entry.selection.taskPacks.length
      || entry.selection.taskPacks.some(value => typeof value !== 'string' || !value)) {
      fail(`${at}.selection.taskPacks`, 'must contain unique non-empty strings');
    }
    for (const field of modular ? ['features', 'checks'] : ['packs', 'checks']) {
      if (!Array.isArray(entry.selection.requested[field])
        || new Set(entry.selection.requested[field]).size !== entry.selection.requested[field].length
        || entry.selection.requested[field].some(value => typeof value !== 'string' || !value)) {
        fail(`${at}.selection.requested.${field}`, 'must contain unique non-empty strings');
      }
    }
    if (modular) {
      strict(entry.selection.requested.specifications,
        `${at}.selection.requested.specifications`, new Set(['disclosed', 'probed']));
      strict(entry.selection.specifications, `${at}.selection.specifications`,
        new Set(['disclosed', 'probed']));
      for (const value of [entry.selection.features, entry.selection.requestedChecks,
        entry.selection.probeChecks]) {
        if (!Array.isArray(value) || new Set(value).size !== value.length
          || value.some(item => typeof item !== 'string' || !item)) {
          fail(`${at}.selection`, 'contains an invalid modular selection array');
        }
      }
      for (const specifications of [entry.selection.requested.specifications,
        entry.selection.specifications]) {
        for (const field of ['disclosed', 'probed']) {
          if (!Array.isArray(specifications[field])
            || new Set(specifications[field]).size !== specifications[field].length
            || specifications[field].some(reference => !REF.test(reference))) {
            fail(`${at}.selection.specifications.${field}`,
              'must contain unique exact id@version references');
          }
        }
      }
    }
    strict(entry.task, `${at}.task`, new Set(['sha256', 'requirementSha256',
      'contractSha256', 'requirementIds', 'contractIds']));
    for (const field of ['sha256', 'requirementSha256', 'contractSha256']) {
      if (!HASH.test(entry.task[field])) fail(`${at}.task.${field}`, 'must be a SHA-256 digest');
    }
    for (const field of ['requirementIds', 'contractIds']) {
      if (!Array.isArray(entry.task[field])
        || new Set(entry.task[field]).size !== entry.task[field].length
        || entry.task[field].some(value => typeof value !== 'string' || !value)) {
        fail(`${at}.task.${field}`, 'must contain unique non-empty strings');
      }
    }
    return entry;
  });
  if (new Set(levels.map(entry => entry.level)).size !== levels.length) {
    fail('requested.levels', 'must not repeat a level');
  }
  return canonicalizeDefinition(input);
}

export function resolveStudyConditions(inputs, stacks,
  { stackBenchRoot = ROOT, catalogPath = CATALOG, frozen = false, requested } = {}) {
  if (!Array.isArray(inputs) || inputs.length === 0) fail('conditions', 'must be a non-empty array');
  const catalog = loadCatalog(catalogPath);
  const resolved = inputs.map((input, index) => {
    const ref = validateConditionReference(input, `conditions[${index}]`);
    const requestedScope = validateRequestedScope(
      typeof requested === 'function' ? requested(ref, index) : requested);
    const guidance = resolveGuidance(catalog, ref.guidanceProfile, stacks, resolve(stackBenchRoot));
    const probes = resolveProbes(catalog, ref.probeProfile);
    const repair = resolveRepair(catalog, ref.repairPolicy);
    if (frozen && [guidance, probes, repair].some(profile => profile.state !== 'qualified')) {
      fail(`conditions[${index}]`, 'cannot freeze with a draft component');
    }
    const content = { id: ref.id, version: ref.version, requested: requestedScope,
      guidance, probes, repair };
    return { ...content, sha256: sha256(canonicalDefinitionJson(content)) };
  });
  const keys = resolved.map(condition => `${condition.id}@${condition.version}`);
  if (new Set(keys).size !== keys.length) fail('conditions', 'must not duplicate id@version');
  return resolved.sort((a, b) => `${a.id}@${a.version}`.localeCompare(`${b.id}@${b.version}`));
}
