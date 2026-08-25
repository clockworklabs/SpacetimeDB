import { existsSync, readFileSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';

import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.mjs';
import { hashFiles, sha256 } from '../evidence/provenance.mjs';
import { STACK_ADAPTER_REGISTRY } from '../stacks/stack-adapters.mjs';

export const QUALIFICATION_SCOPE_SCHEMA_VERSION = 1;

const KINDS = new Set(['reference', 'mutation', 'null']);
const KIND_ENTRYPOINTS = Object.freeze({
  reference: ['commands/bench.mjs', 'src/references/reference-live.mjs'],
  mutation: ['commands/bench.mjs', 'src/references/reference-live.mjs', 'grader/mutation-test.mjs'],
  null: ['commands/null-control.mjs', 'grader/grade.mjs'],
});
const CHILD_ENTRYPOINTS = Object.freeze({
  'commands/bench.mjs': ['commands/run-suite.mjs'],
  'commands/run-suite.mjs': [
    'commands/check-actions.mjs',
    'commands/reset-backend.mjs',
    'grader/grade.mjs',
    'linter/lint.mjs',
  ],
  'grader/mutation-test.mjs': ['grader/grade.mjs'],
});
const RUNTIME_INPUTS = Object.freeze([
  'package.json',
  'package-lock.json',
  'docker-compose.yaml',
  'appliance/Controller.Dockerfile',
  'appliance/docker-compose.yaml',
]);

function fail(message) {
  throw new Error(`qualification scope: ${message}`);
}

function resolveLocalImport(from, specifier, root) {
  const base = resolve(dirname(from), specifier);
  const candidates = extname(base)
    ? [base]
    : [base, `${base}.mjs`, `${base}.js`, `${base}.json`, join(base, 'index.mjs'), join(base, 'index.js')];
  const match = candidates.find(candidate => existsSync(candidate));
  if (!match) fail(`unmapped local import ${specifier} from ${relative(root, from).replaceAll('\\', '/')}`);
  const rel = relative(root, match);
  if (rel === '..' || rel.startsWith(`..\\`) || rel.startsWith('../')) {
    fail(`local import escapes Stack Bench: ${specifier}`);
  }
  return match;
}

function localImports(path, root) {
  if (!/\.(?:mjs|js)$/.test(path)) return [];
  const source = readFileSync(path, 'utf8');
  const imports = [];
  const staticPattern = /\b(?:import|export)\s+(?:[^'";]*?\s+from\s+)?['"]([^'"]+)['"]/g;
  const dynamicPattern = /\bimport\s*\(\s*(['"])([^'"]+)\1\s*\)/g;
  for (const pattern of [staticPattern, dynamicPattern]) {
    for (const match of source.matchAll(pattern)) {
      if (match[pattern === dynamicPattern ? 2 : 1].startsWith('.')) {
        imports.push(resolveLocalImport(path, match[pattern === dynamicPattern ? 2 : 1], root));
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

function moduleGraph(root, entrypoints) {
  const pending = entrypoints.map(path => resolve(root, path));
  const files = new Set();
  while (pending.length) {
    const path = pending.pop();
    if (files.has(path)) continue;
    if (!existsSync(path)) fail(`mapped input does not exist: ${relative(root, path).replaceAll('\\', '/')}`);
    files.add(path);
    pending.push(...localImports(path, root));
    const relativePath = relative(root, path).replaceAll('\\', '/');
    pending.push(...(CHILD_ENTRYPOINTS[relativePath] ?? []).map(child => resolve(root, child)));
  }
  return [...files];
}

function checkIdentity(release) {
  if (!Array.isArray(release?.checkCatalog) || release.checkCatalog.length === 0) {
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
  mutation = null, stackBenchRoot }) {
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

  const files = moduleGraph(root, KIND_ENTRYPOINTS[kind]);
  for (const input of RUNTIME_INPUTS) {
    const path = resolve(root, input);
    if (!existsSync(path)) fail(`mapped runtime input does not exist: ${input}`);
    files.push(path);
  }
  const trackWalk = resolve(root, 'tracks', release.track, 'walk.mjs');
  if (!existsSync(trackWalk)) fail(`mapped track walk does not exist: tracks/${release.track}/walk.mjs`);
  files.push(trackWalk);
  const executable = hashFiles(files, { base: root });
  const adapter = stack === null ? null : STACK_ADAPTER_REGISTRY.get(stack);
  const document = canonicalizeDefinition({
    schemaVersion: QUALIFICATION_SCOPE_SCHEMA_VERSION,
    kind,
    executableSha256: executable.sha256,
    recipe: { id: release.id, version: release.version, contentSha256: release.contentSha256 },
    checksSha256: checkIdentity(release),
    stack: adapter ? { id: adapter.id, version: adapter.version,
      reference: { id: reference.id, sourceSha256: reference.sourceSha256 } } : null,
    mutationSha256: mutation?.executionSha256 ?? null,
  });
  return { ...document, sha256: sha256(canonicalDefinitionJson(document)) };
}

export function validateQualificationScopeIdentity(value, at = 'qualificationScope') {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${at} must be an object`);
  const allowed = new Set(['schemaVersion', 'kind', 'executableSha256', 'recipe', 'checksSha256',
    'stack', 'mutationSha256', 'sha256']);
  for (const key of Object.keys(value)) if (!allowed.has(key)) fail(`${at}.${key} is unknown`);
  if (value.schemaVersion !== QUALIFICATION_SCOPE_SCHEMA_VERSION) fail(`${at}.schemaVersion is unsupported`);
  if (!KINDS.has(value.kind)) fail(`${at}.kind is invalid`);
  const digest = /^[a-f0-9]{64}$/;
  for (const field of ['executableSha256', 'checksSha256', 'sha256']) {
    if (!digest.test(value[field] ?? '')) fail(`${at}.${field} is invalid`);
  }
  const exactFields = (object, fields, nestedAt) => {
    if (!object || typeof object !== 'object' || Array.isArray(object)) fail(`${nestedAt} is invalid`);
    for (const key of Object.keys(object)) if (!fields.has(key)) fail(`${nestedAt}.${key} is unknown`);
  };
  exactFields(value.recipe, new Set(['id', 'version', 'contentSha256']), `${at}.recipe`);
  if (!value.recipe || typeof value.recipe.id !== 'string' || !value.recipe.id
    || typeof value.recipe.version !== 'string' || !value.recipe.version
    || !digest.test(value.recipe.contentSha256 ?? '')) fail(`${at}.recipe is invalid`);
  if (value.kind === 'null') {
    if (value.stack !== null || value.mutationSha256 !== null) fail(`${at} has invalid null scope`);
  } else {
    exactFields(value.stack, new Set(['id', 'version', 'reference']), `${at}.stack`);
    exactFields(value.stack.reference, new Set(['id', 'sourceSha256']), `${at}.stack.reference`);
    if (!value.stack || typeof value.stack.id !== 'string' || !value.stack.id
      || typeof value.stack.version !== 'string' || !value.stack.version
      || !value.stack.reference || typeof value.stack.reference.id !== 'string'
      || !value.stack.reference.id || !digest.test(value.stack.reference.sourceSha256 ?? '')) {
      fail(`${at}.stack is invalid`);
    }
    if (value.kind === 'mutation' ? !digest.test(value.mutationSha256 ?? '')
      : value.mutationSha256 !== null) fail(`${at}.mutationSha256 is invalid`);
  }
  const { sha256: claimed, ...document } = value;
  if (sha256(canonicalDefinitionJson(canonicalizeDefinition(document))) !== claimed) {
    fail(`${at}.sha256 does not match its fields`);
  }
  return structuredClone(value);
}
