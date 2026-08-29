import { readFileSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, relative, resolve, sep } from 'node:path';


import { canonicalDefinitionJson, canonicalizeDefinition } from '../composition/definition-plan.js';
import { normalizePromptText, readAgentSkillDocuments } from '../agents/agent-materials.mjs';
import { sha256 } from '../evidence/provenance.js';
import { validateCredentialAliases } from '../composition/credential-aliases.js';

import { STACK_BENCH_ROOT as ROOT } from '../package-root.js';
const CATALOG = resolve(ROOT, 'conditions', 'catalog.json');
const ID = /^[a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*$/;
const VERSION = /^\d+\.\d+\.\d+$/;
const HASH = /^[a-f0-9]{64}$/;
const REF = /^([a-z][a-z0-9]*(?:[.:-][a-z0-9]+)*)@(\d+\.\d+\.\d+)$/;
type UnknownRecord = Record<string, unknown>;

interface IdentityProfile extends UnknownRecord {
  schemaVersion: number;
  kind: string;
  id: string;
  version: string;
  state: 'draft' | 'qualified';
}

interface Catalog {
  value: { guidanceProfiles: Record<string, string>; repairPolicies: Record<string, string> };
  root: string;
}

interface GuidanceProfile extends IdentityProfile {
  mode: 'prescribed' | 'neutral';
  material: { accessFacts: boolean; apiReference: boolean; designAdvice: boolean };
  documents: Record<string, string>;
  skills: Record<string, string[]>;
  credentialAliases?: unknown;
}

interface ResolvedDocument { path: string; sha256: string; bytes: number }
interface ResolvedSkills { ids: string[]; sha256: string; bytes: number }
export interface ResolvedGuidanceProfile {
  id: string;
  version: string;
  sha256: string;
  state: 'draft' | 'qualified';
  mode: 'prescribed' | 'neutral';
  material: GuidanceProfile['material'];
  documents: Record<string, ResolvedDocument>;
  skills: Record<string, ResolvedSkills>;
  credentialAliases?: Readonly<Record<string, string>>;
}

interface RepairProfile extends IdentityProfile {
  scoredEvidence: boolean;
  observedEvidence: boolean;
}

interface ConditionSpecifications {
  levels: Array<{ level: number; requested: string[]; expected: string[]; observed: string[] }>;
}

export interface ConditionReference {
  id: string;
  version: string;
  guidanceProfile: string;
  repairPolicy: string;
  specifications?: ConditionSpecifications;
}

interface RequestedSelection extends UnknownRecord {
  schemaVersion?: number;
  sha256: string;
  scoredPoints: number;
  completeness?: string;
  requested: UnknownRecord;
  taskPacks?: string[];
  promptPacks?: string[];
  features?: string[];
  specifications?: { requested: string[]; expected: string[]; observed: string[] };
  scoredChecks?: Array<{ stableKey: string; points: number; treatment: string }>;
  observedChecks?: Array<{ stableKey: string; points: number; treatment: string }>;
}

interface RequestedLevel extends UnknownRecord {
  level: number;
  recipe: { id: string; version: string; state: 'draft' | 'qualified'; contentSha256: string;
    meaningSha256: string; executionSha256: string };
  selection: RequestedSelection;
  task: UnknownRecord & { mode?: 'fresh' | 'upgrade'; sha256: string;
    requirementSha256: string; contractSha256: string; requirementIds: string[];
    contractIds: string[] };
}

export interface RequestedScope { track: string; levels: [RequestedLevel, ...RequestedLevel[]] }

export interface ResolvedStudyCondition {
  id: string;
  version: string;
  requested: RequestedScope;
  guidance: ResolvedGuidanceProfile;
  repair: { id: string; version: string; sha256: string; state: 'draft' | 'qualified';
    scoredEvidence: true; observedEvidence: false };
  sha256: string;
}

const object = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);
function fail(at: string, message: string): never {
  throw new Error(`invalid study condition at ${at}: ${message}`);
}

function strict(value: unknown, at: string, fields: Set<string>,
  optional: Set<string> = new Set()): asserts value is UnknownRecord {
  if (!object(value)) throw new Error(`invalid study condition at ${at}: must be an object`);
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'is unknown');
  for (const key of fields) {
    if (!optional.has(key) && !Object.hasOwn(value, key)) fail(`${at}.${key}`, 'is required');
  }
}

function contained(root: string, path: string, at: string): string {
  const absoluteRoot = realpathSync(root);
  const absolute = realpathSync(resolve(absoluteRoot, path));
  const rel = relative(absoluteRoot, absolute);
  if (rel === '..' || rel.startsWith(`..${sep}`) || isAbsolute(rel)) fail(at, 'escapes the condition root');
  return absolute;
}

function json(path: string, at: string): unknown {
  try { return JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) { fail(at, `cannot be read: ${error instanceof Error ? error.message : String(error)}`); }
}

function identity(profile: IdentityProfile, resolved: unknown): {
  id: string; version: string; sha256: string; state: 'draft' | 'qualified' } {
  return { id: profile.id, version: profile.version,
    sha256: sha256(canonicalDefinitionJson({ profile, resolved })), state: profile.state };
}

function validateIdentityFields(value: UnknownRecord, at: string, kind: string,
  schemaVersions: number[] = [1]): void {
  if (typeof value.schemaVersion !== 'number' || !schemaVersions.includes(value.schemaVersion)) {
    fail(`${at}.schemaVersion`, `must be ${schemaVersions.join(' or ')}`);
  }
  if (value.kind !== kind) fail(`${at}.kind`, `must be ${kind}`);
  if (typeof value.id !== 'string' || !ID.test(value.id)) fail(`${at}.id`, 'is invalid');
  if (typeof value.version !== 'string' || !VERSION.test(value.version)) {
    fail(`${at}.version`, 'must be a semantic version');
  }
  if (typeof value.state !== 'string' || !['draft', 'qualified'].includes(value.state)) {
    fail(`${at}.state`, 'must be draft or qualified');
  }
}

function loadCatalog(path: string = CATALOG): Catalog {
  const value = json(path, 'catalog');
  strict(value, 'catalog', new Set(['schemaVersion', 'kind', 'guidanceProfiles',
    'repairPolicies']));
  if (value.schemaVersion !== 1 || value.kind !== 'study-condition-catalog') {
    fail('catalog', 'has an unsupported schema or kind');
  }
  for (const field of ['guidanceProfiles', 'repairPolicies'] as const) {
    const section = value[field];
    if (!object(section)) fail(`catalog.${field}`, 'must be an object');
    for (const [key, pathValue] of Object.entries(section)) {
      if (!REF.test(key) || typeof pathValue !== 'string' || !pathValue) {
        fail(`catalog.${field}.${key}`, 'must map an id@version to a path');
      }
    }
  }
  return { value: value as unknown as Catalog['value'], root: dirname(path) };
}

function readProfile(catalog: Catalog, section: keyof Catalog['value'], reference: string): {
  match: RegExpExecArray; path: string; profile: unknown } {
  const match = REF.exec(reference ?? '');
  if (!match) fail(reference ?? '<missing>', 'must use id@version');
  const rel = catalog.value[section][reference];
  if (!rel) fail(reference, `is not in catalog.${section}`);
  const path = contained(catalog.root, rel, reference);
  const profile = json(path, reference);
  return { match: match!, path, profile };
}

function loadProfile<T extends IdentityProfile>(catalog: Catalog,
  section: keyof Catalog['value'], reference: string, kind: string, fields: Set<string>,
  optional: Set<string> = new Set()): { profile: T; path: string } {
  const { match, path, profile } = readProfile(catalog, section, reference);
  strict(profile, reference, fields, optional);
  validateIdentityFields(profile, reference, kind);
  const typed = profile as T;
  if (typed.id !== match[1] || typed.version !== match[2]) {
    fail(reference, 'does not match the loaded profile identity');
  }
  return { profile: typed, path };
}

function resolveGuidance(catalog: Catalog, reference: string, stacks: readonly string[],
  stackBenchRoot: string): ResolvedGuidanceProfile {
  const fields = new Set(['schemaVersion', 'kind', 'id', 'version', 'state', 'mode', 'material',
    'documents', 'skills', 'credentialAliases']);
  const { profile } = loadProfile<GuidanceProfile>(catalog, 'guidanceProfiles', reference,
    'backend-guidance-profile', fields, new Set(['credentialAliases']));
  if (!['prescribed', 'neutral'].includes(profile.mode)) fail(`${reference}.mode`, 'must be prescribed or neutral');
  strict(profile.material, `${reference}.material`, new Set(['accessFacts', 'apiReference', 'designAdvice']));
  for (const field of ['accessFacts', 'apiReference', 'designAdvice'] as const) {
    if (typeof profile.material[field] !== 'boolean') fail(`${reference}.material.${field}`, 'must be boolean');
  }
  if (profile.mode === 'neutral' && profile.material.designAdvice) {
    fail(`${reference}.material.designAdvice`, 'must be false for neutral guidance');
  }
  if (!object(profile.documents)) fail(`${reference}.documents`, 'must be an object');
  if (!object(profile.skills)) fail(`${reference}.skills`, 'must be an object');
  const documents: Record<string, ResolvedDocument> = {};
  const skills: Record<string, ResolvedSkills> = {};
  for (const stack of stacks) {
    const rel = profile.documents[stack];
    if (typeof rel !== 'string' || !rel) fail(`${reference}.documents.${stack}`, 'is required');
    const path = contained(stackBenchRoot, rel, `${reference}.documents.${stack}`);
    const bytes = Buffer.from(normalizePromptText(readFileSync(path, 'utf8')), 'utf8');
    documents[stack] = { path: relative(stackBenchRoot, path).split(sep).join('/'),
      sha256: sha256(bytes), bytes: bytes.length };
    const ids = profile.skills[stack];
    if (!Array.isArray(ids) || new Set(ids).size !== ids.length
      || ids.some(id => typeof id !== 'string' || !/^[a-z][a-z0-9-]*$/.test(id))) {
      fail(`${reference}.skills.${stack}`, 'must contain unique skill ids');
    }
    const skillIds = ids as string[];
    let text = '';
    try { text = readAgentSkillDocuments(resolve(stackBenchRoot, '..', '..'), skillIds); }
    catch (error) {
      fail(`${reference}.skills.${stack}`,
        `cannot be read: ${error instanceof Error ? error.message : String(error)}`);
    }
    skills[stack] = { ids: [...skillIds], sha256: sha256(text), bytes: Buffer.byteLength(text) };
  }
  const hasCredentialAliases = Object.hasOwn(profile, 'credentialAliases');
  const base = { mode: profile.mode, material: profile.material, documents, skills };
  if (!hasCredentialAliases) return { ...identity(profile, { documents, skills }), ...base };
  const credentialAliases = validateCredentialAliases(profile.credentialAliases,
    `${reference}.credentialAliases`);
  return { ...identity(profile, { documents, skills, credentialAliases }), ...base,
    credentialAliases };
}

// Public review surface for tooling that must inspect the exact guidance
// selected by a profile without manufacturing an otherwise unrelated study
// condition. Campaign compilation uses the same resolver below.
export function resolveGuidanceProfile(reference: string, stacks: readonly string[],
  { stackBenchRoot = ROOT, catalogPath = CATALOG }:
  { stackBenchRoot?: string; catalogPath?: string } = {}): ResolvedGuidanceProfile {
  if (!Array.isArray(stacks) || stacks.length === 0
    || new Set(stacks).size !== stacks.length
    || stacks.some(stack => typeof stack !== 'string' || !ID.test(stack))) {
    fail('stacks', 'must contain unique stack ids');
  }
  return resolveGuidance(loadCatalog(catalogPath), reference, stacks, resolve(stackBenchRoot));
}

function resolveRepair(catalog: Catalog, reference: string): ResolvedStudyCondition['repair'] {
  const fields = new Set(['schemaVersion', 'kind', 'id', 'version', 'state',
    'scoredEvidence', 'observedEvidence']);
  const { profile } = loadProfile<RepairProfile>(catalog, 'repairPolicies', reference,
    'repair-policy', fields);
  if (typeof profile.scoredEvidence !== 'boolean' || typeof profile.observedEvidence !== 'boolean') {
    fail(reference, 'repair evidence fields must be boolean');
  }
  if (!profile.scoredEvidence) {
    fail(`${reference}.scoredEvidence`, 'must be true until no-evidence repair is implemented');
  }
  if (profile.observedEvidence) fail(`${reference}.observedEvidence`, 'must be false');
  return { ...identity(profile, null), scoredEvidence: true, observedEvidence: false };
}

export function validateConditionReference(input: unknown, at = 'condition'): ConditionReference {
  if (!object(input)) fail(at, 'must be an object');
  const value = structuredClone(input) as UnknownRecord;
  const fields = new Set(['id', 'version', 'guidanceProfile', 'repairPolicy',
    'specifications']);
  for (const key of Object.keys(value)) if (!fields.has(key)) fail(`${at}.${key}`, 'is unknown');
  for (const key of ['id', 'version', 'guidanceProfile', 'repairPolicy']) {
    if (!Object.hasOwn(value, key)) fail(`${at}.${key}`, 'is required');
  }
  if (typeof value.id !== 'string' || !ID.test(value.id)) fail(`${at}.id`, 'is invalid');
  if (typeof value.version !== 'string' || !VERSION.test(value.version)) {
    fail(`${at}.version`, 'must be a semantic version');
  }
  for (const field of ['guidanceProfile', 'repairPolicy'] as const) {
    const reference = value[field];
    if (typeof reference !== 'string' || !REF.test(reference)) {
      fail(`${at}.${field}`, 'must use id@version');
    }
  }
  if (value.specifications !== undefined) {
    strict(value.specifications, `${at}.specifications`, new Set(['levels']));
    if (!Array.isArray(value.specifications.levels) || value.specifications.levels.length === 0) {
      fail(`${at}.specifications.levels`, 'must be a non-empty array');
    }
    const specifications = value.specifications as unknown as ConditionSpecifications;
    const seen = new Set<number>();
    specifications.levels.forEach((entry, index) => {
      const levelAt = `${at}.specifications.levels[${index}]`;
      strict(entry, levelAt, new Set(['level', 'requested', 'expected', 'observed']));
      if (!Number.isSafeInteger(entry.level) || entry.level < 1 || seen.has(entry.level)) {
        fail(`${levelAt}.level`, 'must be a unique positive integer');
      }
      seen.add(entry.level);
      for (const field of ['requested', 'expected', 'observed'] as const) {
        if (!Array.isArray(entry[field]) || new Set(entry[field]).size !== entry[field].length
          || entry[field].some(reference => !REF.test(reference))) {
          fail(`${levelAt}.${field}`, 'must contain unique exact id@version references');
        }
        entry[field] = [...entry[field]].sort();
      }
      const treatments = new Map();
      for (const field of ['requested', 'expected', 'observed'] as const) {
        for (const reference of entry[field]) {
          if (treatments.has(reference)) {
            fail(levelAt, `cannot treat ${reference} as both ${treatments.get(reference)} and ${field}`);
          }
          treatments.set(reference, field);
        }
      }
    });
    specifications.levels.sort((left, right) => left.level - right.level);
  }
  return canonicalizeDefinition(value) as unknown as ConditionReference;
}

function validateRequestedScope(input: unknown): RequestedScope {
  strict(input, 'requested', new Set(['track', 'levels']));
  if (typeof input.track !== 'string' || !ID.test(input.track)) {
    fail('requested.track', 'is invalid');
  }
  if (!Array.isArray(input.levels) || input.levels.length === 0) {
    fail('requested.levels', 'must be a non-empty array');
  }
  const scope = input as unknown as RequestedScope;
  const levels = scope.levels.map((entry, index) => {
    const at = `requested.levels[${index}]`;
    strict(entry, at, new Set(['level', 'recipe', 'selection', 'task']));
    if (!Number.isSafeInteger(entry.level) || entry.level < 1) fail(`${at}.level`, 'must be positive');
    strict(entry.recipe, `${at}.recipe`, new Set(['id', 'version', 'contentSha256',
      'meaningSha256', 'executionSha256', 'state']));
    if (!ID.test(entry.recipe.id) || !VERSION.test(entry.recipe.version)
      || !['draft', 'qualified'].includes(entry.recipe.state)
      || (['contentSha256', 'meaningSha256', 'executionSha256'] as const)
        .some(field => !HASH.test(entry.recipe[field]))) {
      fail(`${at}.recipe`, 'has an invalid identity');
    }
    const modular = entry.selection?.schemaVersion === 3;
    strict(entry.selection, `${at}.selection`, modular
      ? new Set(['schemaVersion', 'sha256', 'scoredPoints', 'requested',
        'features', 'specifications', 'promptPacks', 'scoredChecks', 'observedChecks'])
      : new Set(['sha256', 'completeness', 'scoredPoints', 'requested', 'taskPacks']));
    if (!HASH.test(entry.selection.sha256)
      || !Number.isSafeInteger(entry.selection.scoredPoints) || entry.selection.scoredPoints < 0
      || (!modular && (typeof entry.selection.completeness !== 'string'
        || !['full', 'subset'].includes(entry.selection.completeness)))) {
      fail(`${at}.selection`, 'has an invalid identity');
    }
    const requested = structuredClone(entry.selection.requested) as UnknownRecord;
    const dependencyExpansion = requested?.dependencyExpansion;
    if (modular) delete requested.dependencyExpansion;
    strict(requested, `${at}.selection.requested`, modular
      ? new Set(['features', 'specifications', 'checks']) : new Set(['packs', 'checks']));
    if (dependencyExpansion !== undefined && dependencyExpansion !== 'exact') {
      fail(`${at}.selection.requested.dependencyExpansion`, 'must be exact');
    }
    const packs = modular ? entry.selection.promptPacks : entry.selection.taskPacks;
    if (!Array.isArray(packs)
      || new Set(packs).size !== packs.length
      || packs.some(value => typeof value !== 'string' || !value)) {
      fail(`${at}.selection.${modular ? 'promptPacks' : 'taskPacks'}`, 'must contain unique non-empty strings');
    }
    for (const field of modular ? ['features', 'checks'] as const : ['packs', 'checks'] as const) {
      const requestedValues = entry.selection.requested[field];
      if (!Array.isArray(requestedValues)
        || new Set(requestedValues).size !== requestedValues.length
        || requestedValues.some(value => typeof value !== 'string' || !value)) {
        fail(`${at}.selection.requested.${field}`, 'must contain unique non-empty strings');
      }
    }
    if (modular) {
      strict(entry.selection.requested.specifications,
        `${at}.selection.requested.specifications`, new Set(['requested', 'expected', 'observed']));
      strict(entry.selection.specifications, `${at}.selection.specifications`,
        new Set(['requested', 'expected', 'observed']));
      if (!Array.isArray(entry.selection.features)
        || new Set(entry.selection.features).size !== entry.selection.features.length
        || entry.selection.features.some(item => typeof item !== 'string' || !item)) {
        fail(`${at}.selection.features`, 'must contain unique non-empty strings');
      }
      const checkGroups: Array<['scoredChecks' | 'observedChecks', Set<string>]> = [
        ['scoredChecks', new Set(['requested', 'expected'])],
        ['observedChecks', new Set(['observed'])],
      ];
      for (const [field, treatments] of checkGroups) {
        const checks = entry.selection[field];
        if (!Array.isArray(checks)) fail(`${at}.selection.${field}`, 'must be an array');
        const keys = new Set();
        for (const [index, check] of checks.entries()) {
          const checkAt = `${at}.selection.${field}[${index}]`;
          strict(check, checkAt, new Set(['stableKey', 'points', 'treatment']));
          if (typeof check.stableKey !== 'string' || !check.stableKey || keys.has(check.stableKey)) {
            fail(`${checkAt}.stableKey`, 'must be a unique non-empty string');
          }
          keys.add(check.stableKey);
          if (!Number.isSafeInteger(check.points) || check.points < 0) {
            fail(`${checkAt}.points`, 'must be a non-negative integer');
          }
          if (!treatments.has(check.treatment)) {
            fail(`${checkAt}.treatment`, `must be ${[...treatments].join(' or ')}`);
          }
        }
      }
      const requestedSpecifications = entry.selection.requested.specifications;
      if (!object(requestedSpecifications)) {
        fail(`${at}.selection.requested.specifications`, 'must be an object');
      }
      for (const specifications of [requestedSpecifications,
        entry.selection.specifications!]) {
        for (const field of ['requested', 'expected', 'observed'] as const) {
          if (!Array.isArray(specifications[field])
            || new Set(specifications[field]).size !== specifications[field].length
            || specifications[field].some(reference => typeof reference !== 'string'
              || !REF.test(reference))) {
            fail(`${at}.selection.specifications.${field}`,
              'must contain unique exact id@version references');
          }
        }
      }
    }
    strict(entry.task, `${at}.task`, new Set([
      ...(Object.hasOwn(entry.task ?? {}, 'mode') ? ['mode'] : []),
      'sha256', 'requirementSha256', 'contractSha256', 'requirementIds', 'contractIds',
    ]));
    if (entry.task.mode !== undefined && !['fresh', 'upgrade'].includes(entry.task.mode)) {
      fail(`${at}.task.mode`, 'must be fresh or upgrade');
    }
    for (const field of ['sha256', 'requirementSha256', 'contractSha256'] as const) {
      if (!HASH.test(entry.task[field])) fail(`${at}.task.${field}`, 'must be a SHA-256 digest');
    }
    for (const field of ['requirementIds', 'contractIds'] as const) {
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
  return canonicalizeDefinition(scope) as unknown as RequestedScope;
}

export function resolveStudyConditions(inputs: unknown[], stacks: string[],
  { stackBenchRoot = ROOT, catalogPath = CATALOG, frozen = false, requested }:
  { stackBenchRoot?: string; catalogPath?: string; frozen?: boolean;
    requested?: unknown | ((reference: ConditionReference, index: number) => unknown) }
  = {}): [ResolvedStudyCondition, ...ResolvedStudyCondition[]] {
  if (!Array.isArray(inputs) || inputs.length === 0) fail('conditions', 'must be a non-empty array');
  const catalog = loadCatalog(catalogPath);
  const resolved: ResolvedStudyCondition[] = inputs.map((input, index) => {
    const ref = validateConditionReference(input, `conditions[${index}]`);
    const requestedScope = validateRequestedScope(
      typeof requested === 'function' ? requested(ref, index) : requested);
    const guidance = resolveGuidance(catalog, ref.guidanceProfile, stacks, resolve(stackBenchRoot));
    const repair = resolveRepair(catalog, ref.repairPolicy);
    if (frozen && [guidance, repair].some(profile => profile.state !== 'qualified')) {
      fail(`conditions[${index}]`, 'cannot freeze with a draft component');
    }
    const content = { id: ref.id, version: ref.version, requested: requestedScope,
      guidance, repair };
    return { ...content, sha256: sha256(canonicalDefinitionJson(content)) };
  });
  const keys = resolved.map(condition => `${condition.id}@${condition.version}`);
  if (new Set(keys).size !== keys.length) fail('conditions', 'must not duplicate id@version');
  return resolved.sort((a, b) => `${a.id}@${a.version}`
    .localeCompare(`${b.id}@${b.version}`)) as [ResolvedStudyCondition, ...ResolvedStudyCondition[]];
}
