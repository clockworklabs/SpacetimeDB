import assert from 'node:assert/strict';
import test from 'node:test';

import { packageRegistry, packageRegistryEnvironment }
  from '../src/runtime/package-registry.js';
import type { BackendLeaseNetwork } from '../src/runtime/backend-lease.js';

test('the package registry cache is optional and must be a loopback http origin', () => {
  assert.equal(packageRegistry({}), null);
  assert.equal(packageRegistry({ STACK_BENCH_NPM_REGISTRY: 'http://127.0.0.1:4873/' })?.href,
    'http://127.0.0.1:4873/');
  for (const value of ['registry', 'https://127.0.0.1:4873/', 'http://registry.npmjs.org/',
    'http://127.0.0.1/', 'http://127.0.0.1:4873/npm/', 'http://user:pw@127.0.0.1:4873/']) {
    assert.throws(() => packageRegistry({ STACK_BENCH_NPM_REGISTRY: value }), /STACK_BENCH_NPM_REGISTRY/, value);
  }
});

test('coding containers reach the cache through the address their network mode allows', () => {
  const registry = packageRegistry({ STACK_BENCH_NPM_REGISTRY: 'http://127.0.0.1:4873/' });
  assert.deepEqual(packageRegistryEnvironment(registry, 'host'),
    { NPM_CONFIG_REGISTRY: 'http://127.0.0.1:4873/' });
  assert.deepEqual(packageRegistryEnvironment(registry, 'bridge'),
    { NPM_CONFIG_REGISTRY: 'http://host.docker.internal:4873/' });
  assert.deepEqual(packageRegistryEnvironment(null, 'host'), {});
});

test('owned attempts use the same registry origin across fresh cache IP addresses', () => {
  const network: BackendLeaseNetwork = { name: 'attempt', id: 'a'.repeat(64),
    namespaceContainerId: 'b'.repeat(64), cacheContainerId: 'c'.repeat(64),
    hostAddresses: ['172.20.0.1'], services: [], firewallSha256: null, firewallInstalledAt: null };
  for (const address of ['172.20.0.3', '192.168.48.3']) {
    network.services = [{ address, port: 4873 }];
    assert.deepEqual(packageRegistryEnvironment(null, `container:${network.namespaceContainerId}`, network),
      { NPM_CONFIG_REGISTRY: 'http://stack-bench-npm-cache:4873/' });
  }
  assert.throws(() => packageRegistryEnvironment(null, null, { ...network, cacheContainerId: undefined }),
    /authenticated cache connection/);
  assert.throws(() => packageRegistryEnvironment(null, null, { ...network, services: [] }),
    /authenticated cache connection/);
});
