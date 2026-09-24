import { spawnSync } from 'node:child_process';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { expect, test } from 'vitest';

// Checks the built `dist` typings, so run `pnpm build` first (CI does).
test('generated bindings typecheck in a strict consumer', () => {
  const tsc = createRequire(import.meta.url).resolve('typescript/bin/tsc');
  const project = fileURLToPath(
    new URL('./consumer/tsconfig.json', import.meta.url)
  );
  const result = spawnSync(process.execPath, [tsc, '-p', project], {
    encoding: 'utf8',
  });
  expect(result.stdout + result.stderr).toBe('');
}, 60_000);
