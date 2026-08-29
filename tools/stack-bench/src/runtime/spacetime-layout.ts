import { existsSync, lstatSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

const SKIP_DIRECTORIES = new Set([
  '.git', '.vite', 'dist', 'module_bindings', 'node_modules', 'stack-bench',
]);

export class GeneratedAppLayoutError extends Error {
  readonly code = 'generated_app_layout';

  constructor(message: string) {
    super(message);
    this.name = 'GeneratedAppLayoutError';
  }
}

interface ModuleTarget {
  path: string;
  configPath: string | null;
}

export interface SpacetimeModuleLayout {
  moduleDirectory: string;
  hostPath: string;
  containerPath: string;
  configPath: string | null;
  source: 'spacetime.json' | 'required-directory' | 'server-sdk-import';
}

function fail(message: string): never {
  throw new GeneratedAppLayoutError(message);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function readConfig(appRoot: string, path: string): Record<string, unknown> {
  let config: unknown;
  try { config = JSON.parse(readFileSync(path, 'utf8')); }
  catch (error) {
    const detail = error instanceof Error ? error.message : String(error);
    fail(`${relative(appRoot, path)} is not valid JSON: ${detail}`);
  }
  if (!isRecord(config)) {
    fail(`${relative(appRoot, path)} is not valid JSON: expected an object`);
  }
  return config;
}

function inside(root: string, candidate: string): boolean {
  const rel = relative(root, candidate);
  return rel !== '..' && !rel.startsWith(`..${sep}`) && !isAbsolute(rel);
}

function walk(root: string, visit: (path: string, name: string) => void): void {
  for (const entry of readdirSync(root, { withFileTypes: true })) {
    if (entry.isSymbolicLink()) continue;
    const path = join(root, entry.name);
    if (entry.isDirectory()) {
      if (!SKIP_DIRECTORIES.has(entry.name)) walk(path, visit);
      continue;
    }
    if (entry.isFile()) visit(path, entry.name);
  }
}

function configuredPath(appRoot: string, configPath: string, modulePath: string): string {
  const normalized = modulePath.replaceAll('\\', '/');
  if (normalized === '/app') return appRoot;
  if (normalized.startsWith('/app/')) return resolve(appRoot, normalized.slice('/app/'.length));
  return resolve(dirname(configPath), modulePath);
}

function configTargets(appRoot: string): ModuleTarget[] {
  const targets: ModuleTarget[] = [];
  walk(appRoot, (path, name) => {
    if (name !== 'spacetime.json') return;
    const config = readConfig(appRoot, path);
    const modulePath = config['module-path'];
    if (modulePath === undefined) return;
    if (typeof modulePath !== 'string' || !modulePath.trim()) {
      fail(`${relative(appRoot, path)} has an invalid module-path`);
    }
    targets.push({ path: configuredPath(appRoot, path, modulePath), configPath: path });
  });
  return targets;
}

function importsServerSdk(directory: string): boolean {
  const src = join(directory, 'src');
  if (!existsSync(src) || !lstatSync(src).isDirectory()) return false;
  let found = false;
  walk(src, (path, name) => {
    if (found || !/\.(?:[cm]?ts|tsx|[cm]?js|jsx)$/.test(name)) return;
    try { found = /(?:from\s*|require\s*\()['"]spacetimedb\/server['"]/.test(readFileSync(path, 'utf8')); }
    catch { /* an unreadable generated file is not layout evidence */ }
  });
  return found;
}

function discoveredTargets(appRoot: string): ModuleTarget[] {
  const targets: ModuleTarget[] = [];
  walk(appRoot, (path, name) => {
    if (name !== 'package.json') return;
    const directory = dirname(path);
    if (importsServerSdk(directory)) targets.push({ path: directory, configPath: null });
  });
  return targets;
}

function validateTarget(appRoot: string, target: ModuleTarget): Omit<SpacetimeModuleLayout, 'source'> {
  const unresolved = resolve(target.path);
  if (!inside(appRoot, unresolved)) {
    fail(`SpacetimeDB module path escapes the application: ${unresolved}`);
  }
  if (!existsSync(unresolved) || !lstatSync(unresolved).isDirectory()) {
    fail(`SpacetimeDB module directory is missing: ${relative(appRoot, unresolved) || '.'}`);
  }
  const actual = realpathSync(unresolved);
  if (!inside(appRoot, actual)) {
    fail(`SpacetimeDB module path resolves outside the application: ${unresolved}`);
  }
  if (!existsSync(join(actual, 'package.json')) || !importsServerSdk(actual)) {
    fail(`SpacetimeDB module directory is not a TypeScript module: ${relative(appRoot, actual) || '.'}`);
  }
  const moduleDirectory = relative(appRoot, actual).split(sep).join('/');
  return {
    moduleDirectory,
    hostPath: actual,
    containerPath: `/app/${moduleDirectory}`,
    configPath: target.configPath
      ? relative(appRoot, target.configPath).split(sep).join('/') : null,
  };
}

export function resolveSpacetimeModuleLayout(app: string): SpacetimeModuleLayout {
  if (!existsSync(app) || !lstatSync(app).isDirectory()) {
    fail(`application directory is missing: ${resolve(app)}`);
  }
  const appRoot = realpathSync(app);
  const declared = configTargets(appRoot);
  let candidates = declared;
  let source: SpacetimeModuleLayout['source'] = 'spacetime.json';
  if (!candidates.length) {
    const conventional = join(appRoot, 'backend', 'spacetimedb');
    if (existsSync(conventional)) {
      candidates = [{ path: conventional, configPath: null }];
      source = 'required-directory';
    } else {
      candidates = discoveredTargets(appRoot);
      source = 'server-sdk-import';
    }
  }
  const unique = new Map(candidates.map(candidate => [resolve(candidate.path), candidate]));
  if (unique.size === 0) {
    fail('no SpacetimeDB TypeScript module was found; expected backend/spacetimedb or one declared by spacetime.json');
  }
  if (unique.size > 1) {
    fail(`multiple SpacetimeDB module directories were found: ${[...unique.keys()]
      .map(path => relative(appRoot, path)).sort().join(', ')}`);
  }
  const target = unique.values().next().value;
  if (target === undefined) throw new GeneratedAppLayoutError('no SpacetimeDB TypeScript module was found');
  return { ...validateTarget(appRoot, target), source };
}
