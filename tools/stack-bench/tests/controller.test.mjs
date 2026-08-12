import assert from 'node:assert/strict';
import test from 'node:test';

import { resolveControllerCommand } from '../appliance/controller.mjs';

test('controller exposes a small explicit operator command surface', () => {
  assert.equal(resolveControllerCommand([]), null);
  assert.equal(resolveControllerCommand(['--help']), null);
  const run = resolveControllerCommand(['run', '--backend', 'postgres', '--levels', '1-2']);
  assert.equal(run.executable, process.execPath);
  assert.match(run.args[0], /bench\.mjs$/);
  assert.deepEqual(run.args.slice(1), ['--backend', 'postgres', '--levels', '1-2']);
  assert.throws(() => resolveControllerCommand(['shell']), /unknown controller command/);
});
