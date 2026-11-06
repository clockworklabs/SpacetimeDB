#!/usr/bin/env node

import { cpSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

const root = resolve(import.meta.dirname, '..', '..');
const source = join(root, 'dashboard', 'public');
const target = join(root, 'dist', 'dashboard', 'public');

mkdirSync(dirname(target), { recursive: true });
cpSync(source, target, { recursive: true, filter: path => !path.endsWith('.ts') });
