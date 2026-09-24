import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

// Checks the built `dist` typings, so run `pnpm build` first (CI does).
test.each(['tsconfig.json', 'nodenext/tsconfig.json'])(
  'a strict consumer typechecks against the package (%s)',
  config => {
    const tsc = createRequire(import.meta.url).resolve('typescript/bin/tsc');
    const project = fileURLToPath(
      new URL(`./consumer/${config}`, import.meta.url)
    );
    const result = spawnSync(process.execPath, [tsc, '-p', project], {
      encoding: 'utf8',
    });
    expect(result.stdout + result.stderr).toBe('');
    expect(result.status).toBe(0);
  },
  60_000
);
