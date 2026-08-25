#!/usr/bin/env node

import { existsSync, readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];
let checked = 0;

for (const entry of readdirSync(root, { withFileTypes: true })) {
  if (!entry.isDirectory() || !/^spacetime-.+-ts$/.test(entry.name)) continue;

  const publicDir = join(root, entry.name, 'example', 'public');
  const indexPath = join(publicDir, 'index.html');
  if (!existsSync(indexPath)) continue;

  checked++;
  const html = readFileSync(indexPath, 'utf8');
  const stylesPath = join(publicDir, 'styles.css');
  const uiPath = join(publicDir, 'ui.js');

  if (/<style(?:\s|>)/i.test(html)) {
    failures.push(`${entry.name}: index.html contains an inline style block`);
  }
  if (/<script(?![^>]*\bsrc=)[^>]*>/i.test(html)) {
    failures.push(`${entry.name}: index.html contains an inline script block`);
  }
  if (!existsSync(stylesPath)) {
    failures.push(`${entry.name}: public/styles.css is missing`);
  }
  if (!/<link[^>]+href=["'](?:\.\/|\/)styles\.css["'][^>]*>/i.test(html)) {
    failures.push(`${entry.name}: index.html does not load styles.css`);
  }

  const loadsUi = /<script[^>]+src=["']\.\/ui\.js["'][^>]*>/i.test(html);
  if (existsSync(uiPath) !== loadsUi) {
    failures.push(
      `${entry.name}: public/ui.js and its index.html script tag do not match`
    );
  }
}

if (checked !== 12) {
  failures.push(`expected 12 browser examples, found ${checked}`);
}

if (failures.length > 0) {
  console.error('Example asset check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Example asset check passed for ${checked} browser examples.`);
