import assert from 'node:assert/strict';
import test from 'node:test';

import { packageRegistry, packageRegistryEnvironment, packageRegistryPort }
  from '../src/runtime/package-registry.js';

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
  assert.equal(packageRegistryPort(registry), 4873);
  assert.equal(packageRegistryPort(null), null);
});
