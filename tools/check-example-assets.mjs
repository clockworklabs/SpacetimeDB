#!/usr/bin/env node

import { existsSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { releasePackages } from './release-packages.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const failures = [];
let checked = 0;

for (const packageDir of releasePackages) {
  const exampleDir = join(root, packageDir, 'example');
  if (!existsSync(join(exampleDir, 'package.json'))) continue;

  const publicDir = join(exampleDir, 'public');
  const indexPath = join(publicDir, 'index.html');
  if (!existsSync(indexPath)) {
    failures.push(`${packageDir}: example/public/index.html is missing`);
    continue;
  }

  checked++;
  const html = readFileSync(indexPath, 'utf8');
  const stylesPath = join(publicDir, 'styles.css');
  const uiPath = join(publicDir, 'ui.js');

  if (/<style(?:\s|>)/i.test(html)) {
    failures.push(`${packageDir}: index.html contains an inline style block`);
  }
  if (/<script(?![^>]*\bsrc=)[^>]*>/i.test(html)) {
    failures.push(`${packageDir}: index.html contains an inline script block`);
  }
  if (!existsSync(stylesPath)) {
    failures.push(`${packageDir}: public/styles.css is missing`);
  }
  if (!/<link[^>]+href=["'](?:\.\/|\/)styles\.css["'][^>]*>/i.test(html)) {
    failures.push(`${packageDir}: index.html does not load styles.css`);
  }

  const loadsUi = /<script[^>]+src=["']\.\/ui\.js["'][^>]*>/i.test(html);
  if (existsSync(uiPath) !== loadsUi) {
    failures.push(
      `${packageDir}: public/ui.js and its index.html script tag do not match`
    );
  }
}

if (failures.length > 0) {
  console.error('Example asset check failed:');
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log(`Example asset check passed for ${checked} browser examples.`);
