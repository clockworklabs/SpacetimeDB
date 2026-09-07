import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { dbName, moduleName } from '../src/composition/tracks.js';
import { loadTrack } from '../src/composition/tracks.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { CODING_CONTAINER_CONTROL_DIR, CODING_CONTAINER_PROCESS_IDENTITY,
  codingContainerAgentCommand, codingContainerAgentEnvironment,
  codingContainerAgentExecOptions, codingContainerTranscriptHandoffCommands,
  codingContainerWorkspaceHandoffCommands }
  from '../src/runtime/coding-container-policy.js';
import { leasedDatabaseEnvironment, STACK_ADAPTER_REGISTRY }
  from '../src/stacks/stack-adapters.js';
import { POSTGRES_APPLICATION_IDENTITY, attemptDatabaseIdentity, attemptDatabaseUrl }
  from '../src/stacks/hosted-database-identity.js';
import { spacetimeBuildContainerPlan } from '../src/stacks/stack-agent-operations.js';

const FORBIDDEN_IDENTITY = /stackbench|stack[-_ ]bench|benchmark|harness|test|grader/i;

test('workspace handoff keeps the agent owner and gives the controller group access', () => {
  assert.deepEqual(codingContainerWorkspaceHandoffCommands(42), [
    ['chown', '-R', '10001:42', '/app'],
    ['chmod', '-R', 'u+rwX,g+rwX,o-rwx', '/app'],
  ]);
});

test('transcript handoff gives only the controller group read access', () => {
  assert.deepEqual(codingContainerTranscriptHandoffCommands(42), [
    ['chown', '-R', '10001:42', '/home/developer/.claude/projects/-app'],
    ['chmod', '-R', 'u+rwX,g+rX,o-rwx', '/home/developer/.claude/projects/-app'],
  ]);
});

test('the materialized agent runtime uses neutral application identities', () => {
  const track = { ...loadTrack('ecommerce'), slug: 'ecom' };
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
    spacetimeConfigTarget: (() => {
      const mount = spacetimePlan.mounts[0];
      assert(mount);
      return mount.target;
    })(),
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

test('source PostgreSQL defaults and private attempt URLs keep neutral application identities', () => {
  const expected = POSTGRES_APPLICATION_IDENTITY;
  for (const relativePath of ['docker-compose.yaml']) {
    const compose = readFileSync(join(STACK_BENCH_ROOT, relativePath), 'utf8');
    assert.match(compose, new RegExp(`POSTGRES_USER: ${expected.user}\\b`));
    assert.match(compose, new RegExp(`POSTGRES_PASSWORD: ${expected.password}\\b`));
    assert.match(compose, new RegExp(`POSTGRES_DB: ${expected.defaultDatabase}\\b`));
    assert.match(compose,
      new RegExp(`pg_isready -U ${expected.user} -d ${expected.defaultDatabase}`));
  }
  const token = 'a'.repeat(32);
  const identity = attemptDatabaseIdentity(token);
  const postgres = new URL(attemptDatabaseUrl({ backend: 'postgres', database: 'app_run1', ownershipToken: token }));
  const mongo = new URL(attemptDatabaseUrl({ backend: 'mongodb', database: 'app_run1', ownershipToken: token }));
  for (const url of [postgres, mongo]) {
    assert.equal(url.username, expected.user);
    assert.equal(url.hostname, '127.0.0.1');
    assert.equal(url.password, identity.password);
    assert.equal(url.pathname, '/app_run1');
    assert.doesNotMatch(url.href, FORBIDDEN_IDENTITY);
  }
  assert.equal(postgres.port, '5432');
  assert.equal(mongo.port, '27017');
  assert.notEqual(identity.password, attemptDatabaseIdentity('b'.repeat(32)).password);
  assert.notEqual(identity.password, identity.adminPassword);
  assert.doesNotMatch(JSON.stringify(expected), FORBIDDEN_IDENTITY);
});
