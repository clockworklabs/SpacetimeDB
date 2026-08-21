import { existsSync, lstatSync, readFileSync, readdirSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep } from 'node:path';

const SKIP_DIRECTORIES = new Set([
  '.git', '.vite', 'dist', 'module_bindings', 'node_modules', 'stack-bench',
]);

export class GeneratedAppLayoutError extends Error {
  constructor(message) {
    super(message);
    this.name = 'GeneratedAppLayoutError';
    this.code = 'generated_app_layout';
  }
}

const fail = message => { throw new GeneratedAppLayoutError(message); };

function inside(root, candidate) {
  const rel = relative(root, candidate);
  return rel !== '..' && !rel.startsWith(`..${sep}`) && !isAbsolute(rel);
}

function walk(root, visit) {
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

function configuredPath(appRoot, configPath, modulePath) {
  const normalized = modulePath.replaceAll('\\', '/');
  if (normalized === '/app') return appRoot;
  if (normalized.startsWith('/app/')) return resolve(appRoot, normalized.slice('/app/'.length));
  return resolve(dirname(configPath), modulePath);
}

function configTargets(appRoot) {
  const targets = [];
  walk(appRoot, (path, name) => {
    if (name !== 'spacetime.json') return;
    let config;
    try { config = JSON.parse(readFileSync(path, 'utf8')); }
    catch (error) { fail(`${relative(appRoot, path)} is not valid JSON: ${error.message}`); }
    if (config['module-path'] === undefined) return;
    if (typeof config['module-path'] !== 'string' || !config['module-path'].trim()) {
      fail(`${relative(appRoot, path)} has an invalid module-path`);
    }
    targets.push({ path: configuredPath(appRoot, path, config['module-path']), configPath: path });
  });
  return targets;
}

function importsServerSdk(directory) {
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

function discoveredTargets(appRoot) {
  const targets = [];
  walk(appRoot, (path, name) => {
    if (name !== 'package.json') return;
    const directory = dirname(path);
    if (importsServerSdk(directory)) targets.push({ path: directory, configPath: null });
  });
  return targets;
}

function validateTarget(appRoot, target) {
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

export function resolveSpacetimeModuleLayout(app) {
  if (!existsSync(app) || !lstatSync(app).isDirectory()) {
    fail(`application directory is missing: ${resolve(app)}`);
  }
  const appRoot = realpathSync(app);
  const declared = configTargets(appRoot);
  let candidates = declared;
  let source = 'spacetime.json';
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
  return { ...validateTarget(appRoot, [...unique.values()][0]), source };
}
