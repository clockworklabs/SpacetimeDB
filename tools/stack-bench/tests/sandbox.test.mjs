import assert from 'node:assert/strict';
import test from 'node:test';

import { sandboxProbeMode } from '../sandbox.mjs';

test('appliance isolation replaces the single-host model CLI sandbox probe', () => {
  assert.equal(sandboxProbeMode({ appliance: true, stackRequired: true }), 'container-isolation');
  assert.equal(sandboxProbeMode({ appliance: false, stackRequired: true }), 'direct-cli');
  assert.equal(sandboxProbeMode({ appliance: true, stackRequired: false }), 'not-required');
  assert.equal(sandboxProbeMode({ appliance: true, stackRequired: true, explicitlySkipped: true }),
    'explicitly-skipped');
});
