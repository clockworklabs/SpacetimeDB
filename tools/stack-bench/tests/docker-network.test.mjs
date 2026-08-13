import assert from 'node:assert/strict';
import test from 'node:test';

import { dockerHostGatewayArguments } from '../docker-network.mjs';

test('bridge containers bind the portable Docker host gateway alias', () => {
  assert.deepEqual(dockerHostGatewayArguments('bridge'),
    ['--add-host', 'host.docker.internal:host-gateway']);
  assert.deepEqual(dockerHostGatewayArguments('host'), []);
  assert.throws(() => dockerHostGatewayArguments('ambient'), /unsupported Docker network mode/);
});
