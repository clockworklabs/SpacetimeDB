import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { dbName, moduleName } from '../src/composition/tracks.mjs';
import { STACK_BENCH_ROOT } from '../src/project-paths.mjs';
import { CODING_CONTAINER_CONTROL_DIR, CODING_CONTAINER_PROCESS_IDENTITY,
  codingContainerAgentCommand, codingContainerAgentEnvironment,
  codingContainerAgentExecOptions } from '../src/runtime/coding-container-policy.mjs';
import { leasedDatabaseEnvironment, STACK_ADAPTER_REGISTRY }
  from '../src/stacks/stack-adapters.mjs';
import { POSTGRES_APPLICATION_IDENTITY }
  from '../src/stacks/hosted-database-identity.mjs';
import { spacetimeBuildContainerPlan } from '../src/stacks/stack-agent-operations.mjs';

const FORBIDDEN_IDENTITY = /stackbench|stack[-_ ]bench|benchmark|harness|test|grader/i;

test('the materialized agent runtime uses neutral application identities', () => {
  const track = { slug: 'ecom' };
  const database = dbName(track, 4);
  const spacetimePlan = spacetimeBuildContainerPlan({
    repo: 'C:/product-source', appDir: 'C:/workspace/app',
    env: { STACK_BENCH_RELEASE_DEPS_VOLUME: 'release-dependencies' },
  });
  const visibleRuntime = {
    environment: codingContainerAgentEnvironment(),
    execOptions: codingContainerAgentExecOptions(),
    execCommand: codingContainerAgentCommand('claude', ['--print']),
    controlDirectory: CODING_CONTAINER_CONTROL_DIR,
    processIdentity: CODING_CONTAINER_PROCESS_IDENTITY,
    database,
    module: moduleName(track, 4),
    postgres: leasedDatabaseEnvironment(STACK_ADAPTER_REGISTRY.get('postgres'), {
      database, networkMode: 'bridge',
    }),
    mongodb: leasedDatabaseEnvironment(STACK_ADAPTER_REGISTRY.get('mongodb'), {
      database, networkMode: 'bridge',
    }),
    spacetimeConfigTarget: spacetimePlan.mounts[0].target,
  };

  assert.doesNotMatch(JSON.stringify(visibleRuntime), FORBIDDEN_IDENTITY);
  assert.deepEqual(visibleRuntime.environment,
    { HOME: '/home/developer', USER: 'developer' });
  assert.equal(visibleRuntime.postgres.DATABASE_URL,
    'postgresql://appuser:local-app-password@host.docker.internal:6532/app_ecom_run4');
  assert.equal(visibleRuntime.mongodb.DATABASE_URL,
    'mongodb://host.docker.internal:6537/app_ecom_run4');
  assert.equal(visibleRuntime.module, 'app-ecom-run4');
});

test('PostgreSQL services and generated connection URLs use one neutral identity', () => {
  const expected = POSTGRES_APPLICATION_IDENTITY;
  for (const relativePath of ['docker-compose.yaml', 'appliance/docker-compose.yaml']) {
    const compose = readFileSync(join(STACK_BENCH_ROOT, relativePath), 'utf8');
    assert.match(compose, new RegExp(`POSTGRES_USER: ${expected.user}\\b`));
    assert.match(compose, new RegExp(`POSTGRES_PASSWORD: ${expected.password}\\b`));
    assert.match(compose, new RegExp(`POSTGRES_DB: ${expected.defaultDatabase}\\b`));
    assert.match(compose,
      new RegExp(`pg_isready -U ${expected.user} -d ${expected.defaultDatabase}`));
  }
  assert.doesNotMatch(JSON.stringify(expected), FORBIDDEN_IDENTITY);
});
