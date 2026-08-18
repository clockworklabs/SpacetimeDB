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

function modulesBelow(directory, output = []) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    if (entry.isDirectory() && SKIPPED_DIRECTORIES.has(entry.name)) continue;
    const path = join(directory, entry.name);
    if (entry.isDirectory()) modulesBelow(path, output);
    else if (entry.isFile() && extname(entry.name) === '.mjs') output.push(path);
  }
  return output;
}

test('the project root contains no implementation modules', () => {
  const rootModules = readdirSync(ROOT, { withFileTypes: true })
    .filter(entry => entry.isFile() && entry.name.endsWith('.mjs'))
    .map(entry => entry.name);
  assert.deepEqual(rootModules, []);
  assert.deepEqual(readdirSync(join(ROOT, 'src'), { withFileTypes: true })
    .filter(entry => entry.isDirectory()).map(entry => entry.name).sort(), SOURCE_AREAS);
});

test('production-relative module references resolve after source reorganization', () => {
  const missing = [];
  const relativeModule = /(?:from\s+|import\s*\()(['"])(\.\.?\/[^'"\r\n]+?\.mjs)\1/g;
  for (const file of modulesBelow(ROOT)) {
    const source = readFileSync(file, 'utf8');
    for (const match of source.matchAll(relativeModule)) {
      const target = resolve(dirname(file), match[2]);
      if (!existsSync(target)) missing.push(`${relative(ROOT, file)} -> ${match[2]}`);
    }
  }
  assert.deepEqual(missing, []);
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
