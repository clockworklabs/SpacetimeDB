import assert from 'node:assert/strict';
import test from 'node:test';

import { databaseContainerName } from '../src/stacks/database-containers.mjs';

test('database container defaults isolate development from appliance services', () => {
  assert.equal(databaseContainerName('postgres', {}), 'stack-bench-dev-postgres');
  assert.equal(databaseContainerName('mongodb', {}), 'stack-bench-dev-mongodb');
  assert.equal(databaseContainerName('postgres', { STACK_BENCH_APPLIANCE: '1' }),
    'stack-bench-postgres');
  assert.equal(databaseContainerName('mongodb', { STACK_BENCH_APPLIANCE: '1' }),
    'stack-bench-mongodb');
});

test('explicit database container names override topology defaults', () => {
  assert.equal(databaseContainerName('postgres', { POSTGRES_CONTAINER: 'custom-pg' }), 'custom-pg');
  assert.equal(databaseContainerName('mongodb', { MONGO_CONTAINER: 'custom-mongo' }), 'custom-mongo');
});
