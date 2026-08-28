#!/usr/bin/env node

import { copyFileSync, mkdirSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..');
const output = join(root, 'dist');
const sourceDirectories = ['appliance', 'commands', 'container', 'dashboard', 'grader', 'linter', 'src'];

function copyModules(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const source = join(directory, entry.name);
    if (entry.isDirectory()) {
      copyModules(source);
      continue;
    }
    if (!entry.isFile() || !entry.name.endsWith('.mjs')) continue;
    const target = join(output, relative(root, source));
    mkdirSync(dirname(target), { recursive: true });
    copyFileSync(source, target);
  }
}

for (const directory of sourceDirectories) copyModules(join(root, directory));
