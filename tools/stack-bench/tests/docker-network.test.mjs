import assert from 'node:assert/strict';
import test from 'node:test';

import { dockerHostGatewayArguments, dockerHostServiceAddress } from '../src/runtime/docker-network.mjs';

test('bridge containers bind the portable Docker host gateway alias', () => {
  assert.deepEqual(dockerHostGatewayArguments('bridge'),
    ['--add-host', 'host.docker.internal:host-gateway']);
  assert.deepEqual(dockerHostGatewayArguments('host'), []);
  assert.throws(() => dockerHostGatewayArguments('ambient'), /unsupported Docker network mode/);
});

test('host services use the address reachable from the selected network namespace', () => {
  assert.equal(dockerHostServiceAddress('bridge'), 'host.docker.internal');
  assert.equal(dockerHostServiceAddress('host'), '127.0.0.1');
  assert.throws(() => dockerHostServiceAddress('ambient'), /unsupported Docker network mode/);
});
