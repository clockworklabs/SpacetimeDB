#!/usr/bin/env node
// Checks that the plugin's skills copy matches the repository's skills/ directory
// the copy cannot be a symlink, plugin installers materialize the payload without
// following symlinks, so a symlinked skills directory installs empty
//
// Usage:
//   node codex-plugin/scripts/check-skills-sync.ts         exit 1 on drift
//   node codex-plugin/scripts/check-skills-sync.ts --fix   resync the copy from skills/
//
// Runs directly on Node 22.18+ via native type stripping, no build step needed.

import { promises as fs } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot: string = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  '../..'
);
const source: string = path.join(repoRoot, 'skills');
const copy: string = path.join(
  repoRoot,
  'codex-plugin',
  'plugins',
  'spacetimedb',
  'skills'
);

async function walk(dir: string, base: string = dir): Promise<string[]> {
  const out: string[] = [];
  for (const entry of await fs.readdir(dir, { withFileTypes: true })) {
    const abs = path.join(dir, entry.name);
    if (entry.isDirectory()) out.push(...(await walk(abs, base)));
    else out.push(path.relative(base, abs));
  }
  return out.sort();
}

async function main(): Promise<void> {
  if (process.argv.includes('--fix')) {
    await fs.rm(copy, { recursive: true, force: true });
    await fs.cp(source, copy, { recursive: true });
    console.log('synced skills/ -> codex-plugin/plugins/spacetimedb/skills');
  }

  const [sourceFiles, copyFiles] = await Promise.all([
    walk(source),
    walk(copy),
  ]);
  const drift: string[] = [];

  for (const file of sourceFiles) {
    if (!copyFiles.includes(file)) {
      drift.push(`missing from copy: ${file}`);
    } else {
      const [a, b] = await Promise.all([
        fs.readFile(path.join(source, file)),
        fs.readFile(path.join(copy, file)),
      ]);
      if (!a.equals(b)) drift.push(`differs: ${file}`);
    }
  }
  for (const file of copyFiles) {
    if (!sourceFiles.includes(file)) drift.push(`extraneous in copy: ${file}`);
  }

  if (drift.length > 0) {
    console.error('plugin skills are out of sync with skills/:');
    for (const line of drift) console.error(`  ${line}`);
    console.error(
      '\nrun: node codex-plugin/scripts/check-skills-sync.ts --fix'
    );
    process.exit(1);
  }
  console.log(`skills in sync (${sourceFiles.length} files)`);
}

main().catch((err: unknown) => {
  console.error(err);
  process.exit(1);
});
