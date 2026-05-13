#!/usr/bin/env node
/**
 * Post-build script: copies the plugin-generated llms.txt to static/llms.md
 * so it can be committed to the repo.
 *
 * Usage: pnpm build && node scripts/generate-llms.mjs
 *    or: pnpm generate-llms
 */
import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const BUILD_DIR = path.resolve(__dirname, '../build');
const STATIC_DIR = path.resolve(__dirname, '../static');

async function findInBuild(filename) {
  for (const candidate of [filename, `docs/${filename}`]) {
    const p = path.join(BUILD_DIR, candidate);
    try {
      await fs.access(p);
      return p;
    } catch {}
  }
  return null;
}

async function main() {
  const src = await findInBuild('llms.txt');
  if (!src) {
    console.error('Error: llms.txt not found in build output.');
    console.error('Run "pnpm build" first to generate it via the plugin.');
    process.exit(1);
  }

  const content = await fs.readFile(src, 'utf8');
  const dest = path.join(STATIC_DIR, 'llms.md');
  await fs.writeFile(dest, content, 'utf8');

  const lines = content.split('\n').length;
  console.log(`${src} -> ${dest}`);
  console.log(`  ${lines} lines`);
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
