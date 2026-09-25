import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';
import { z } from 'zod';

import { canonicalDefinitionJson, canonicalizeDefinition } from './definition-plan.js';
import { hashFiles, sha256 } from '../evidence/provenance.js';
import { STACK_IDS, stackAdapterVersion, stackApplicationInterface } from '../stacks/stack-identities.js';
import { interfaceBlocksText } from './agent-visible-contract.js';
import { formatZodError } from '../zod-error.js';

export const QUALIFICATION_SCOPE_SCHEMA_VERSION = 3;

export type QualificationKind = 'reference' | 'mutation' | 'null';

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
  contentSha256: string;
  track: string;
  checkCatalog: QualificationCheck[];
  task?: { contracts: Array<{ path: string }> };
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
  recipe: { contentSha256: string; id: string };
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
const DIGEST = /^[a-f0-9]{64}$/;
const qualificationScopeSchema = z.strictObject({
  schemaVersion: z.literal(QUALIFICATION_SCOPE_SCHEMA_VERSION),
  kind: z.enum(['reference', 'mutation', 'null']),
  executableSha256: z.string().regex(DIGEST),
  recipe: z.strictObject({
    id: z.string().min(1), contentSha256: z.string().regex(DIGEST),
  }),
  checksSha256: z.string().regex(DIGEST),
  stack: z.strictObject({
    id: z.string().min(1),
    version: z.string().min(1),
    reference: z.strictObject({ id: z.string().min(1), sourceSha256: z.string().regex(DIGEST) }),
  }).nullable(),
  mutationSha256: z.string().regex(DIGEST).nullable(),
  sha256: z.string().regex(DIGEST),
});
const KIND_ENTRYPOINTS: Readonly<Record<QualificationKind, readonly string[]>> = Object.freeze({
  reference: ['commands/bench.ts', 'src/references/reference-live.ts'],
  mutation: ['commands/bench.ts', 'src/references/reference-live.ts', 'grader/mutation-test.ts'],
  null: ['commands/null-control.ts', 'grader/grade.ts'],
});
const CHILD_ENTRYPOINTS: Readonly<Record<string, readonly string[]>> = Object.freeze({
  'commands/bench.ts': ['commands/run-suite.ts'],
  'src/references/reference-live.ts': ['src/references/reference-agent.ts'],
  'src/references/reference-agent.ts': ['container/run-build.ts'],
  'commands/run-suite.ts': [
    'commands/check-actions.ts',
    'commands/reset-backend.ts',
    'grader/grade.ts',
    'linter/lint.ts',
  ],
  'grader/mutation-test.ts': ['grader/grade.ts'],
  'src/actions/network-interruption.ts': ['container/browser-network-proxy.ts'],
  'src/stacks/backends/spacetime-browser-session.ts': [
    'dist/src/stacks/spacetime-wire-codec.js',
    'src/stacks/spacetime-wire-codec.entry.mjs',
    'scripts/build-spacetime-codec.mjs',
  ],
});
// A file under this root belongs to the stack named before the first '-' of its
// name, or of its top directory for files a stack reads at runtime. Another stack's
// files never enter a scope, so adding a stack changes no other stack's scope.
const STACK_OWNED_ROOT = 'src/stacks/backends/';
const BACKEND_ONLY_MODULES = new Set(['src/stacks/stack-adapters.ts']);
// Registries hold one registration per stack. A scope hashes their shared lines and
// its own stack's registrations only.
const REGISTRY_MODULES = new Set(['src/stacks/stack-adapters.ts', 'src/stacks/stack-identities.ts']);
const REGISTRATION = /^\s*(?:\[\s*'[a-z0-9]+',\s*)?(?:\{[^{}]*\}|[A-Za-z_$][\w$]*)\s*\]?,$/;
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

function resolveLocalImport(from: string, specifier: string, root: string): string {
  const base = resolve(dirname(from), specifier);
  const extension = extname(base);
  const candidates = extension
    ? [base, ...(extension === '.js' ? [`${base.slice(0, -extension.length)}.ts`] : [])]
    : [base, `${base}.js`, `${base}.ts`, `${base}.json`,
      join(base, 'index.js'), join(base, 'index.ts')];
  const match = candidates.find(candidate => existsSync(candidate));
  if (!match) fail(`unmapped local import ${specifier} from ${relative(root, from).replaceAll('\\', '/')}`);
  const rel = relative(root, match);
  if (rel === '..' || rel.startsWith(`..\\`) || rel.startsWith('../')) {
    fail(`local import escapes Stack Bench: ${specifier}`);
  }
  return match;
}

function localImports(path: string, root: string): string[] {
  if (!/\.(?:js|ts)$/.test(path)) return [];
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
  const declaredTrackLoader = relativePath === 'linter/lint.ts'
    && source.includes(trackLoaderCall);
  // The recovery worker imports this same module. Its source and imports are
  // already in the graph; reject a changed worker target instead of omitting it.
  const selfLoaderCall = ['import', '(workerData.module)'].join('');
  const declaredRecoveryLoader = relativePath === 'src/runtime/backend-control.ts'
    && source.includes(selfLoaderCall)
    && source.includes('workerData: { module: import.meta.url, spec, target }');
  // The SDK codec is bundled at build time. Hash the executable bundle above,
  // and reject any loader target that does not match that declared dependency.
  const codecLoaderCall = ['import', "(new URL('../spacetime-wire-codec.js', import.meta.url).href)"].join('');
  const declaredCodecLoader = relativePath === 'src/stacks/backends/spacetime-browser-session.ts'
    && source.includes(codecLoaderCall);
  if (dynamicCalls !== literalDynamicCalls + (declaredTrackLoader ? 1 : 0)
    + (declaredRecoveryLoader ? 1 : 0) + (declaredCodecLoader ? 1 : 0)) {
    fail(`unmapped dynamic import in ${relativePath}`);
  }
  return imports;
}

function moduleOwner(relativePath: string): string | null {
  if (BACKEND_ONLY_MODULES.has(relativePath)) return '*';
  if (!relativePath.startsWith(STACK_OWNED_ROOT)) return null;
  const owner = /^([a-z0-9]+)(?:-|\/)/.exec(relativePath.slice(STACK_OWNED_ROOT.length))?.[1];
  if (!owner || !STACK_IDS.includes(owner)) fail(`stack-owned module ${relativePath} names no registered stack`);
  return owner;
}

// A stack's runtime assets: every file under src/stacks/backends/<stack>/.
function stackAssets(root: string, stack: string): string[] {
  const directory = resolve(root, STACK_OWNED_ROOT, stack);
  if (!existsSync(directory)) return [];
  return readdirSync(directory, { recursive: true, withFileTypes: true })
    .filter(entry => entry.isFile()).map(entry => join(entry.parentPath, entry.name));
}

// Drops other stacks' registrations. Any other line that uses another stack's
// symbols is refused, so logic cannot hide behind a registration.
function registryProjection(path: string, root: string, stack: string | null): string {
  const owners = new Map<string, string>();
  const kept: string[] = [];
  const relativePath = relative(root, path).replaceAll('\\', '/');
  for (const line of readFileSync(path, 'utf8').replaceAll('\r\n', '\n').split('\n')) {
    const imported = /^import[ ](?:\{([^}]+)\} from )?'(\.\/backends\/[^']+)';$/.exec(line);
    if (imported) {
      const owner = moduleOwner(relative(root, resolveLocalImport(path, imported[2]!, root)).replaceAll('\\', '/'))!;
      for (const name of (imported[1] ?? '').split(',').map(part => part.trim()).filter(Boolean)) owners.set(name, owner);
      if (owner === stack) kept.push(line);
      continue;
    }
    const words = new Set(line.match(/[A-Za-z_$][\w$]*/g) ?? []);
    const foreign = new Set([...owners].filter(([name]) => words.has(name)).map(([, owner]) => owner));
    if (foreign.size === 0 || (foreign.size === 1 && foreign.has(stack ?? ''))) { kept.push(line); continue; }
    if (foreign.size > 1 || !REGISTRATION.test(line)) fail(`${relativePath} uses another stack outside a registration: ${line.trim()}`);
  }
  return kept.join('\n');
}

// The contract text only this stack sees: its interface blocks in the recipe's contracts.
function stackInterfaceText(root: string, release: QualificationRelease, stack: string): string {
  if (!release.task?.contracts) fail('stack evidence requires the recipe contract fragments');
  const selected = stackApplicationInterface(stack);
  return [...new Set(release.task.contracts.map(fragment => fragment.path))].sort().map(path => {
    const file = resolve(root, 'tracks', release.track, path);
    if (!existsSync(file)) fail(`mapped contract does not exist: tracks/${release.track}/${path}`);
    return `${path}\n${interfaceBlocksText(readFileSync(file, 'utf8').replaceAll('\r\n', '\n'), selected)}`;
  }).join('\n');
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
    const imports = localImports(path, root);
    // A stack's modules may not reach into another stack's: that module would be
    // pruned from this scope, so a change to it could not invalidate this stack.
    if (owner && owner !== '*') {
      for (const imported of imports) {
        const importedOwner = moduleOwner(relative(root, imported).replaceAll('\\', '/'));
        if (importedOwner && importedOwner !== '*' && importedOwner !== owner) {
          fail(`${relativePath} imports a module owned by ${importedOwner}; move shared code outside ${STACK_OWNED_ROOT}`);
        }
      }
    }
    pending.push(...imports);
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
  if (!release?.id || !release?.contentSha256 || !release?.track) {
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

  const files = moduleGraph(root, [...KIND_ENTRYPOINTS[kind],
    ...(stack === null ? [] : stackAssets(root, stack).map(path => relative(root, path)))], { stack });
  const registries = files.filter(path => REGISTRY_MODULES.has(relative(root, path).replaceAll('\\', '/')));
  for (const input of RUNTIME_INPUTS) {
    const path = resolve(root, input);
    if (!existsSync(path)) fail(`mapped runtime input does not exist: ${input}`);
    files.push(path);
  }
  const trackWalk = resolve(root, 'tracks', release.track, 'walk.ts');
  if (!existsSync(trackWalk)) fail(`mapped track walk does not exist: tracks/${release.track}/walk.ts`);
  files.push(trackWalk);
  const hashed = hashFiles(files.filter(path => !registries.includes(path)), { base: root, lineEndings: 'lf' });
  const executable = stack === null && registries.length === 0 ? hashed : { sha256: sha256(canonicalDefinitionJson({
    files: hashed.sha256,
    registries: registries.map(path => ({ name: relative(root, path).replaceAll('\\', '/'),
      sha256: sha256(registryProjection(path, root, stack)) })).sort((a, b) => a.name.localeCompare(b.name)),
    ...(stack === null ? {} : { interface: sha256(stackInterfaceText(root, release, stack)) }),
  })) };
  const adapter = stack === null ? null : { id: stack, version: stackAdapterVersion(stack) };
  const document: QualificationScopeDocument = {
    checksSha256: checkIdentity(release),
    executableSha256: executable.sha256,
    kind,
    mutationSha256: mutation?.executionSha256 ?? null,
    recipe: { contentSha256: release.contentSha256, id: release.id },
    schemaVersion: QUALIFICATION_SCOPE_SCHEMA_VERSION,
    stack: adapter && reference ? { id: adapter.id,
      reference: { id: reference.id, sourceSha256: reference.sourceSha256 },
      version: adapter.version } : null,
  };
  return { ...document, sha256: sha256(canonicalDefinitionJson(document)) };
}

function assertQualificationScopeIdentity(value: unknown, at: string):
  asserts value is QualificationScopeIdentity {
  const parsed = qualificationScopeSchema.safeParse(value);
  if (!parsed.success) fail(formatZodError(parsed.error, at));
  const scope = parsed.data;
  if (scope.kind === 'null') {
    if (scope.stack !== null || scope.mutationSha256 !== null) fail(`${at} has invalid null scope`);
  } else {
    if (!scope.stack) fail(`${at}.stack is invalid`);
    if (scope.kind === 'mutation' ? scope.mutationSha256 === null
      : scope.mutationSha256 !== null) fail(`${at}.mutationSha256 is invalid`);
  }
  const { sha256: claimed, ...document } = scope;
  if (sha256(canonicalDefinitionJson(canonicalizeDefinition(document))) !== claimed) {
    fail(`${at}.sha256 does not match its fields`);
  }
}

export function validateQualificationScopeIdentity(value: unknown,
  at = 'qualificationScope'): QualificationScopeIdentity {
  assertQualificationScopeIdentity(value, at);
  return structuredClone(value);
}
