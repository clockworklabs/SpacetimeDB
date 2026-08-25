#!/usr/bin/env node

import { spawnSync } from 'node:child_process';
import { spacetimedbVersion } from './release-packages.mjs';

const command = process.platform === 'win32' ? 'spacetime.exe' : 'spacetime';
const result = spawnSync(command, ['--version'], {
  encoding: 'utf8',
  shell: false,
});

if (result.error) {
  console.error(`Could not run ${command}: ${result.error.message}`);
  console.error(
    'Install the released CLI from https://spacetimedb.com/install.'
  );
  process.exit(1);
}

if (result.status !== 0) {
  console.error(
    result.stderr || result.stdout || `${command} exited with ${result.status}`
  );
  process.exit(result.status ?? 1);
}

const output = `${result.stdout}\n${result.stderr}`.trim();
const toolVersion = output.match(/spacetimedb tool version\s+([^\s;]+)/)?.[1];
const libVersion = output.match(/spacetimedb-lib version\s+([^\s;]+)/)?.[1];

if (toolVersion !== spacetimedbVersion || libVersion !== spacetimedbVersion) {
  console.error(`Expected SpacetimeDB tool and library ${spacetimedbVersion}.`);
  console.error(output || 'The CLI did not report version information.');
  console.error(`Run: spacetime version install ${spacetimedbVersion}`);
  console.error(`Then: spacetime version use ${spacetimedbVersion}`);
  process.exit(1);
}

console.log(`SpacetimeDB released toolchain ${spacetimedbVersion} is active.`);
