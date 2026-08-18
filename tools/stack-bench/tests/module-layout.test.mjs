import assert from 'node:assert/strict';
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, extname, join, relative, resolve } from 'node:path';
import test from 'node:test';

const ROOT = join(import.meta.dirname, '..');
const SOURCE_AREAS = [
  'actions', 'agents', 'campaigns', 'composition', 'evidence', 'references',
  'releases', 'runtime', 'stacks',
];
const SKIPPED_DIRECTORIES = new Set([
  'archive', 'local-notes', 'node_modules', 'results', 'tests',
]);
const RELATIVE_MODULE = /(?:from\s+|import\s*\()(['"])(\.\.?\/[^'"\r\n]+?\.mjs)\1/g;

function modulesBelow(directory, output = []) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && SKIPPED_DIRECTORIES.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) modulesBelow(path, output);
    else if (entry.isFile() && extname(entry.name) === '.mjs') output.push(path);
  }
  return output;
}

function relativeModuleTargets(file) {
  return [...readFileSync(file, 'utf8').matchAll(RELATIVE_MODULE)]
    .map(match => resolve(dirname(file), match[2]));
}

test('the project root contains no implementation modules', () => {
  const rootModules = readdirSync(ROOT, { withFileTypes: true })
    .filter(entry => entry.isFile() && entry.name.endsWith('.mjs'))
    .map(entry => entry.name);
  assert.deepEqual(rootModules, []);
  assert.deepEqual(readdirSync(join(ROOT, 'src'), { withFileTypes: true })
    .filter(entry => entry.isDirectory()).map(entry => entry.name).sort(), SOURCE_AREAS);
});

test('tracked production areas contain no tmp-named scratch modules', () => {
  const scratch = modulesBelow(ROOT)
    .map(path => relative(ROOT, path))
    .filter(path => /(?:^|[\\/])tmp-/.test(path));
  assert.deepEqual(scratch, []);
});

test('production-relative module references resolve after source reorganization', () => {
  const missing = [];
  for (const file of modulesBelow(ROOT)) {
    for (const target of relativeModuleTargets(file)) {
      if (!existsSync(target)) missing.push(`${relative(ROOT, file)} -> ${relative(dirname(file), target)}`);
    }
  }
  assert.deepEqual(missing, []);
});

test('production libraries do not import command entrypoints', () => {
  const violations = [];
  for (const area of ['src', 'grader', 'dashboard']) {
    const directory = join(ROOT, area);
    if (!existsSync(directory)) continue;
    for (const file of modulesBelow(directory)) {
      const source = readFileSync(file, 'utf8');
      if (/from\s+['"][^'"]*commands\/|import\s*\(['"][^'"]*commands\//.test(source)) {
        violations.push(relative(ROOT, file));
      }
    }
  }
  assert.deepEqual(violations, []);
});

test('production modules have no circular imports', () => {
  const files = modulesBelow(ROOT);
  const known = new Set(files);
  const graph = new Map(files.map(file => [file,
    relativeModuleTargets(file).filter(target => known.has(target))]));
  const visited = new Set();
  const active = new Set();
  const stack = [];
  function visit(file) {
    if (active.has(file)) {
      const cycle = [...stack.slice(stack.indexOf(file)), file]
        .map(path => relative(ROOT, path)).join(' -> ');
      assert.fail(`circular import: ${cycle}`);
    }
    if (visited.has(file)) return;
    visited.add(file);
    active.add(file);
    stack.push(file);
    for (const target of graph.get(file)) visit(target);
    stack.pop();
    active.delete(file);
  }
  for (const file of files) visit(file);
});

test('package command entrypoints exist', () => {
  const pkg = JSON.parse(readFileSync(join(ROOT, 'package.json'), 'utf8'));
  for (const [name, command] of Object.entries(pkg.scripts)) {
    const match = command.match(/^node\s+([^\s]+\.mjs)(?:\s|$)/);
    if (!match) continue;
    assert.equal(existsSync(join(ROOT, match[1])), true,
      `${name} points to missing entrypoint ${match[1]}`);
  }
});

test('local and appliance compose files pin the same database images and volumes', () => {
  const local = readFileSync(join(ROOT, 'docker-compose.yaml'), 'utf8');
  const appliance = readFileSync(join(ROOT, 'appliance', 'docker-compose.yaml'), 'utf8');
  const image = (source, service) => source.match(new RegExp(
    `^  ${service}:\\r?\\n(?:    .*\\r?\\n)*?    image: (\\S+)$`, 'm'))?.[1] ?? null;
  for (const service of ['postgres', 'mongodb']) {
    assert.match(image(local, service) ?? '', /@sha256:[a-f0-9]{64}$/);
    assert.equal(image(local, service), image(appliance, service));
  }
  for (const volume of ['stack-bench-appliance-pgdata', 'stack-bench-appliance-mongodata']) {
    assert.match(local, new RegExp(`name: ${volume}`));
    assert.match(appliance, new RegExp(`name: ${volume}`));
  }
});
