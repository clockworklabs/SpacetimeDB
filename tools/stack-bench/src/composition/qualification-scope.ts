import { existsSync, readFileSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';

import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.js';
import { hashFiles, sha256 } from '../evidence/provenance.js';
import { stackAdapterVersion } from '../stacks/stack-identities.js';

export const QUALIFICATION_SCOPE_SCHEMA_VERSION = 2;

export type QualificationKind = 'reference' | 'mutation' | 'null';
type UnknownRecord = Record<string, unknown>;

interface QualificationCheck {
  stableKey: string;
  executionId: unknown;
  source?: unknown;
  featureId?: unknown;
  criterionId?: unknown;
  points: unknown;
}

interface QualificationRelease {
  id: string;
  version: string;
  contentSha256: string;
  track: string;
  checkCatalog: QualificationCheck[];
}

interface QualificationReference {
  backend: string;
  id: string;
  sourceSha256: string;
}

interface QualificationMutation {
  backend: string;
  executionSha256?: string;
}

interface QualificationStackIdentity {
  id: string;
  reference: { id: string; sourceSha256: string };
  version: string;
}

interface QualificationScopeDocument {
  checksSha256: string;
  executableSha256: string;
  kind: QualificationKind;
  mutationSha256: string | null;
  recipe: { contentSha256: string; id: string; version: string };
  schemaVersion: number;
  stack: QualificationStackIdentity | null;
}

export interface QualificationScopeIdentity extends QualificationScopeDocument {
  sha256: string;
}

export interface QualificationScopeInput {
  kind: QualificationKind;
  release: QualificationRelease;
  stack?: string | null;
  reference?: QualificationReference | null;
  mutation?: QualificationMutation | null;
  stackBenchRoot: string;
}

const KINDS = new Set<QualificationKind>(['reference', 'mutation', 'null']);
const qualificationKind = (value: string): value is QualificationKind =>
  value === 'reference' || value === 'mutation' || value === 'null';
const KIND_ENTRYPOINTS: Readonly<Record<QualificationKind, readonly string[]>> = Object.freeze({
  reference: ['commands/bench.mjs', 'src/references/reference-live.mjs'],
  mutation: ['commands/bench.mjs', 'src/references/reference-live.mjs', 'grader/mutation-test.mjs'],
  null: ['commands/null-control.mjs', 'grader/grade.mjs'],
});
const CHILD_ENTRYPOINTS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  'commands/bench.mjs': ['commands/run-suite.mjs'],
  'src/references/reference-live.mjs': ['src/references/reference-agent.mjs'],
  'src/references/reference-agent.mjs': ['container/run-build.mjs'],
  'commands/run-suite.mjs': [
    'commands/check-actions.mjs',
    'commands/reset-backend.mjs',
    'grader/grade.mjs',
    'linter/lint.mjs',
  ],
  'grader/mutation-test.mjs': ['grader/grade.mjs'],
});
const STACK_OWNED_MODULES = new Map<string, string>([
  ['src/stacks/backends/mongodb-adapter.mjs', 'mongodb'],
  ['src/stacks/backends/mongodb-identity.ts', 'mongodb'],
  ['src/stacks/backends/mongodb-operations.mjs', 'mongodb'],
  ['src/stacks/backends/postgres-adapter.mjs', 'postgres'],
  ['src/stacks/backends/postgres-identity.ts', 'postgres'],
  ['src/stacks/backends/postgres-operations.mjs', 'postgres'],
  ['src/stacks/backends/spacetime-adapter.mjs', 'spacetime'],
  ['src/stacks/backends/spacetime-identity.ts', 'spacetime'],
  ['src/stacks/backends/spacetime-operations.mjs', 'spacetime'],
  ['src/stacks/backends/stub-adapter.mjs', 'stub'],
  ['src/stacks/backends/stub-identity.ts', 'stub'],
]);
const STACK_OWNED_ROOT = 'src/stacks/backends/';
const BACKEND_ONLY_MODULES = new Set(['src/stacks/stack-adapters.ts']);
const RUNTIME_INPUTS = Object.freeze([
  'package.json',
  'package-lock.json',
  'docker-compose.yaml',
  'appliance/Controller.Dockerfile',
  'appliance/docker-compose.yaml',
]);

function fail(message: string): never {
  throw new Error(`qualification scope: ${message}`);
}

const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function resolveLocalImport(from: string, specifier: string, root: string): string {
  const base = resolve(dirname(from), specifier);
  const extension = extname(base);
  const candidates = extension
    ? [base, ...(extension === '.js' ? [`${base.slice(0, -extension.length)}.ts`] : [])]
    : [base, `${base}.mjs`, `${base}.js`, `${base}.ts`, `${base}.json`,
      join(base, 'index.mjs'), join(base, 'index.js'), join(base, 'index.ts')];
  const match = candidates.find(candidate => existsSync(candidate));
  if (!match) fail(`unmapped local import ${specifier} from ${relative(root, from).replaceAll('\\', '/')}`);
  const rel = relative(root, match);
  if (rel === '..' || rel.startsWith(`..\\`) || rel.startsWith('../')) {
    fail(`local import escapes Stack Bench: ${specifier}`);
  }
  return match;
}

function localImports(path: string, root: string): string[] {
  if (!/\.(?:mjs|js|ts)$/.test(path)) return [];
  const source = readFileSync(path, 'utf8');
  const imports: string[] = [];
  const staticPattern = /\b(?:import|export)\s+(?:[^'";]*?\s+from\s+)?['"]([^'"]+)['"]/g;
  const dynamicPattern = /\bimport\s*\(\s*(['"])([^'"]+)\1\s*\)/g;
  for (const pattern of [staticPattern, dynamicPattern]) {
    for (const match of source.matchAll(pattern)) {
      if (/^(?:import|export)\s+type\b/.test(match[0])) continue;
      const specifier = match[pattern === dynamicPattern ? 2 : 1];
      if (specifier?.startsWith('.')) {
        imports.push(resolveLocalImport(path, specifier, root));
      }
    }
  }
  const dynamicCalls = source.match(/\bimport\s*\(/g)?.length ?? 0;
  const literalDynamicCalls = [...source.matchAll(dynamicPattern)].length;
  const relativePath = relative(root, path).replaceAll('\\', '/');
  const trackLoaderCall = ['import', '(pathToFileURL(track.walk).href)'].join('');
  const declaredTrackLoader = relativePath === 'linter/lint.mjs'
    && source.includes(trackLoaderCall);
  if (dynamicCalls !== literalDynamicCalls + (declaredTrackLoader ? 1 : 0)) {
    fail(`unmapped dynamic import in ${relativePath}`);
  }
  return imports;
}

function moduleOwner(relativePath: string): string | null {
  if (BACKEND_ONLY_MODULES.has(relativePath)) return '*';
  if (!relativePath.startsWith(STACK_OWNED_ROOT)) return null;
  const owner = STACK_OWNED_MODULES.get(relativePath);
  if (!owner) fail(`unmapped stack-owned module ${relativePath}`);
  return owner;
}

function moduleGraph(root: string, entrypoints: readonly string[], {
  stack,
}: { stack: string | null }): string[] {
  const pending = entrypoints.map(path => resolve(root, path));
  const files = new Set<string>();
  while (pending.length) {
    const path = pending.pop();
    if (!path) continue;
    if (files.has(path)) continue;
    const relativePath = relative(root, path).replaceAll('\\', '/');
    if (!existsSync(path)) fail(`mapped input does not exist: ${relativePath}`);
    const owner = moduleOwner(relativePath);
    if (owner && (stack === null || (owner !== '*' && owner !== stack))) continue;
    files.add(path);
    pending.push(...localImports(path, root));
    pending.push(...(CHILD_ENTRYPOINTS[relativePath] ?? []).map(child => resolve(root, child)));
  }
  return [...files];
}

function checkIdentity(release: QualificationRelease): string {
  if (!Array.isArray(release.checkCatalog) || release.checkCatalog.length === 0) {
    fail('a resolved recipe check catalog is required');
  }
  const checks = release.checkCatalog.map(check => ({
    stableKey: check.stableKey,
    executionId: check.executionId,
    source: check.source,
    featureId: check.featureId,
    criterionId: check.criterionId,
    points: check.points,
  })).sort((a, b) => a.stableKey.localeCompare(b.stableKey));
  if (checks.some(check => typeof check.stableKey !== 'string' || !check.stableKey)) {
    fail('every selected check requires a stable key');
  }
  if (new Set(checks.map(check => check.stableKey)).size !== checks.length) {
    fail('selected check stable keys must be unique');
  }
  return sha256(canonicalDefinitionJson(canonicalizeDefinition(checks)));
}

export function qualificationScopeIdentity({ kind, release, stack = null, reference = null,
  mutation = null, stackBenchRoot }: QualificationScopeInput): QualificationScopeIdentity {
  if (!KINDS.has(kind)) fail(`unknown evidence kind ${JSON.stringify(kind)}`);
  const root = resolve(stackBenchRoot);
  if (!existsSync(root)) fail(`Stack Bench root does not exist: ${root}`);
  if (!release?.id || !release?.version || !release?.contentSha256 || !release?.track) {
    fail('a resolved recipe release is required');
  }
  if (kind === 'null') {
    if (stack !== null || reference !== null || mutation !== null) {
      fail('null evidence cannot declare a stack, reference, or mutation');
    }
  } else {
    if (typeof stack !== 'string' || !stack) fail(`${kind} evidence requires a stack`);
    if (!reference || reference.backend !== stack || !reference.id || !reference.sourceSha256) {
      fail(`${kind} evidence requires its exact stack reference`);
    }
    if (kind === 'mutation' && (!mutation || mutation.backend !== stack
      || !mutation.executionSha256)) {
      fail('mutation evidence requires its exact stack mutation input');
    }
    if (kind === 'reference' && mutation !== null) fail('reference evidence cannot declare a mutation');
  }

  const files = moduleGraph(root, KIND_ENTRYPOINTS[kind], { stack });
  for (const input of RUNTIME_INPUTS) {
    const path = resolve(root, input);
    if (!existsSync(path)) fail(`mapped runtime input does not exist: ${input}`);
    files.push(path);
  }
  const trackWalk = resolve(root, 'tracks', release.track, 'walk.mjs');
  if (!existsSync(trackWalk)) fail(`mapped track walk does not exist: tracks/${release.track}/walk.mjs`);
  files.push(trackWalk);
  const executable = hashFiles(files, { base: root });
  const adapter = stack === null ? null : { id: stack, version: stackAdapterVersion(stack) };
  const document: QualificationScopeDocument = {
    checksSha256: checkIdentity(release),
    executableSha256: executable.sha256,
    kind,
    mutationSha256: mutation?.executionSha256 ?? null,
    recipe: { contentSha256: release.contentSha256, id: release.id, version: release.version },
    schemaVersion: QUALIFICATION_SCOPE_SCHEMA_VERSION,
    stack: adapter && reference ? { id: adapter.id,
      reference: { id: reference.id, sourceSha256: reference.sourceSha256 },
      version: adapter.version } : null,
  };
  return { ...document, sha256: sha256(canonicalDefinitionJson(document)) };
}

function assertQualificationScopeIdentity(value: unknown, at: string):
  asserts value is QualificationScopeIdentity {
  if (!record(value)) fail(`${at} must be an object`);
  const allowed = new Set(['schemaVersion', 'kind', 'executableSha256', 'recipe', 'checksSha256',
    'stack', 'mutationSha256', 'sha256']);
  for (const key of Object.keys(value)) if (!allowed.has(key)) fail(`${at}.${key} is unknown`);
  if (value.schemaVersion !== QUALIFICATION_SCOPE_SCHEMA_VERSION) fail(`${at}.schemaVersion is unsupported`);
  if (typeof value.kind !== 'string' || !qualificationKind(value.kind)) {
    fail(`${at}.kind is invalid`);
  }
  const digest = /^[a-f0-9]{64}$/;
  for (const field of ['executableSha256', 'checksSha256', 'sha256']) {
    const candidate = value[field];
    if (typeof candidate !== 'string' || !digest.test(candidate)) fail(`${at}.${field} is invalid`);
  }
  function exactFields(object: unknown, fields: ReadonlySet<string>, nestedAt: string):
    asserts object is UnknownRecord {
    if (!record(object)) fail(`${nestedAt} is invalid`);
    for (const key of Object.keys(object)) if (!fields.has(key)) fail(`${nestedAt}.${key} is unknown`);
  }
  exactFields(value.recipe, new Set(['id', 'version', 'contentSha256']), `${at}.recipe`);
  if (typeof value.recipe.id !== 'string' || !value.recipe.id
    || typeof value.recipe.version !== 'string' || !value.recipe.version
    || typeof value.recipe.contentSha256 !== 'string'
    || !digest.test(value.recipe.contentSha256)) fail(`${at}.recipe is invalid`);
  if (value.kind === 'null') {
    if (value.stack !== null || value.mutationSha256 !== null) fail(`${at} has invalid null scope`);
  } else {
    exactFields(value.stack, new Set(['id', 'version', 'reference']), `${at}.stack`);
    exactFields(value.stack.reference, new Set(['id', 'sourceSha256']), `${at}.stack.reference`);
    if (typeof value.stack.id !== 'string' || !value.stack.id
      || typeof value.stack.version !== 'string' || !value.stack.version
      || typeof value.stack.reference.id !== 'string'
      || !value.stack.reference.id || typeof value.stack.reference.sourceSha256 !== 'string'
      || !digest.test(value.stack.reference.sourceSha256)) {
      fail(`${at}.stack is invalid`);
    }
    if (value.kind === 'mutation' ? typeof value.mutationSha256 !== 'string'
      || !digest.test(value.mutationSha256)
      : value.mutationSha256 !== null) fail(`${at}.mutationSha256 is invalid`);
  }
  const { sha256: claimed, ...document } = value;
  if (sha256(canonicalDefinitionJson(canonicalizeDefinition(document))) !== claimed) {
    fail(`${at}.sha256 does not match its fields`);
  }
}

export function validateQualificationScopeIdentity(value: unknown,
  at = 'qualificationScope'): QualificationScopeIdentity {
  assertQualificationScopeIdentity(value, at);
  return structuredClone(value);
}
