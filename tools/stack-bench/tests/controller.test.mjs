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
  const recovery = resolveControllerCommand(['recover', '/private/supervisor.json']);
  assert.match(recovery.args[0], /recovery\.mjs$/);
  assert.deepEqual(recovery.args.slice(1), ['recover', '/private/supervisor.json']);
  const campaign = resolveControllerCommand(['campaign', 'show', '/plans/campaign.json']);
  assert.match(campaign.args[0], /campaign-cli\.mjs$/);
  assert.deepEqual(campaign.args.slice(1), ['show', '/plans/campaign.json']);
  const campaignRun = resolveControllerCommand(['campaign', 'run', '/plans/campaign.json',
    '--out', '/results/campaign-001']);
  assert.deepEqual(campaignRun.args.slice(1), ['run', '/plans/campaign.json',
    '--out', '/results/campaign-001']);
  const reference = resolveControllerCommand(['qualify-reference', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '1']);
  assert.match(reference.args[0], /reference-live\.mjs$/);
  const nullControl = resolveControllerCommand(['qualify-null', '--track', 'ecommerce', '--level', '1']);
  assert.match(nullControl.args[0], /null-control\.mjs$/);
  const qualification = resolveControllerCommand(['qualification', 'status',
    '--track', 'ecommerce', '--level', '1']);
  assert.match(qualification.args[0], /qualification-cli\.mjs$/);
  assert.throws(() => resolveControllerCommand(['shell']), /unknown controller command/);
});
