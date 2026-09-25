import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { qualificationScopeIdentity, validateQualificationScopeIdentity }
  from '../src/composition/qualification-scope.js';
import type { QualificationKind } from '../src/composition/qualification-scope.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

type TestStack = 'mongodb' | 'postgres';

const digest = (character: string): string => character.repeat(64);
const release = {
  id: 'ecommerce.l1', contentSha256: digest('a'), track: 'ecommerce',
  checkCatalog: [
    { stableKey: 'check.a', executionId: 'suite', source: 'scenarios/a.json',
      featureId: '1', criterionId: 'a', points: 1 },
    { stableKey: 'check.b', executionId: 'suite', source: 'scenarios/b.json',
      featureId: '1', criterionId: 'b', points: 2 },
  ],
  task: { contracts: [] as Array<{ path: string }> },
};
const references: Record<TestStack, { backend: TestStack; id: string; sourceSha256: string }> = {
  mongodb: { backend: 'mongodb', id: 'mongo-reference', sourceSha256: digest('b') },
  postgres: { backend: 'postgres', id: 'postgres-reference', sourceSha256: digest('c') },
};

test('qualification scopes resolve the current executable tree for every real stack', () => {
  const nullScope = qualificationScopeIdentity({ kind: 'null', release, stackBenchRoot: STACK_BENCH_ROOT });
  assert.deepEqual(validateQualificationScopeIdentity(nullScope), nullScope);
  for (const stack of ['postgres', 'mongodb', 'spacetime', 'convex']) {
    for (const kind of ['reference', 'mutation'] as const) {
      const identity = qualificationScopeIdentity({ kind, release, stack, stackBenchRoot: STACK_BENCH_ROOT,
        reference: { backend: stack, id: `reference-${stack}`, sourceSha256: digest('b') },
        ...(kind === 'mutation' ? { mutation: { backend: stack, executionSha256: digest('c') } } : {}) });
      assert.deepEqual(validateQualificationScopeIdentity(identity), identity);
    }
  }
});
const mutations: Record<TestStack, { backend: TestStack; executionSha256: string }> = {
  mongodb: { backend: 'mongodb', executionSha256: digest('d') },
  postgres: { backend: 'postgres', executionSha256: digest('e') },
};

test('SpacetimeDB replay qualification includes its executable codec and rejects missing or changed loaders', () => {
  const root = fixture();
  const replay = 'src/stacks/backends/spacetime-browser-session.ts';
  const bundle = 'dist/src/stacks/spacetime-wire-codec.js';
  const scope = () => qualificationScopeIdentity({ kind: 'reference', release,
    stack: 'spacetime', reference: { backend: 'spacetime', id: 'reference', sourceSha256: digest('b') },
    stackBenchRoot: root });
  try {
    write(root, 'grader/grade.ts', "import '../src/stacks/backends/spacetime-browser-session.js';\n");
    write(root, replay, "import(new URL('../spacetime-wire-codec.js', import.meta.url).href);\n");
    write(root, 'scripts/build-spacetime-codec.mjs', 'codec build');
    write(root, 'src/stacks/spacetime-wire-codec.entry.mjs', 'codec entry');
    assert.throws(scope, /mapped input does not exist/);
    write(root, bundle, 'export const codec = 1;');
    const before = scope();
    const mongo = scoped(root, 'reference', 'mongodb');
    write(root, bundle, 'export const codec = 2;');
    assert.notEqual(scope().sha256, before.sha256);
    assert.deepEqual(scoped(root, 'reference', 'mongodb'), mongo);
    write(root, replay, "import(new URL('../other-codec.js', import.meta.url).href);\n");
    assert.throws(scope, /unmapped dynamic import/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

function write(root: string, path: string, source = ''): void {
  const target = join(root, path);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, source);
}

function fixture(): string {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-qualification-scope-'));
  for (const path of [
    'commands/run-suite.ts', 'commands/check-actions.ts', 'commands/reset-backend.ts',
    'commands/bench.ts',
    'commands/null-control.ts', 'src/references/reference-live.ts',
    'src/references/reference-agent.ts', 'container/run-build.ts', 'grader/grade.ts',
    'container/browser-network-proxy.ts', 'src/actions/network-interruption.ts',
    'grader/mutation-test.ts', 'linter/lint.ts', 'package.json', 'package-lock.json',
    'docker-compose.yaml', 'appliance/Controller.Dockerfile', 'appliance/docker-compose.yaml',
    'tracks/ecommerce/walk.ts', 'src/evidence/provenance.ts',
  ]) write(root, path, `${path}\n`);
  write(root, 'src/references/reference-live.ts',
    "import '../evidence/provenance.js';\n");
  write(root, 'commands/bench.ts', "import '../src/stacks/stack-adapters.js';\n");
  write(root, 'grader/grade.ts',
    "import '../src/stacks/stack-adapters.js';\nimport '../src/actions/network-interruption.js';\n");
  write(root, 'src/stacks/stack-adapters.ts', [
    "import './backends/mongodb-adapter.js';",
    "import './backends/mongodb-identity.js';",
    "import './backends/mongodb-operations.js';",
    "import './backends/postgres-adapter.js';",
    "import './backends/postgres-identity.js';",
    "import './backends/postgres-operations.js';",
    "import './backends/spacetime-adapter.js';",
    "import './backends/spacetime-identity.js';",
    "import './backends/spacetime-operations.js';",
    '',
  ].join('\n'));
  write(root, 'src/stacks/stack-adapter-common.ts', 'shared adapter helpers\n');
  for (const stack of ['mongodb', 'postgres', 'spacetime']) {
    write(root, `src/stacks/backends/${stack}-adapter.ts`,
      `import '../stack-adapter-common.js';\n${stack} adapter\n`);
    write(root, `src/stacks/backends/${stack}-identity.ts`, `${stack} identity\n`);
    write(root, `src/stacks/backends/${stack}-operations.ts`, `${stack} operations\n`);
  }
  return root;
}

function scoped(root: string, kind: QualificationKind, stack: TestStack | null = null,
  changedRelease = release) {
  return qualificationScopeIdentity({
    kind,
    release: changedRelease,
    stack,
    reference: stack === null ? null : references[stack],
    mutation: kind === 'mutation' && stack !== null ? mutations[stack] : null,
    stackBenchRoot: root,
  });
}

test('qualification identities isolate stack, mutation, and selected-check inputs', () => {
  const root = fixture();
  try {
    const mongoReference = scoped(root, 'reference', 'mongodb');
    const postgresReference = scoped(root, 'reference', 'postgres');
    const mongoMutation = scoped(root, 'mutation', 'mongodb');
    assert.notEqual(mongoReference.executableSha256, postgresReference.executableSha256);
    assert.equal(mongoReference.checksSha256, postgresReference.checksSha256);
    assert.notDeepEqual(mongoReference.stack, postgresReference.stack);
    assert.notEqual(mongoReference.sha256, postgresReference.sha256);
    assert.notEqual(mongoReference.executableSha256, mongoMutation.executableSha256);

    const changedChecks = structuredClone(release);
    const firstCheck = changedChecks.checkCatalog[0];
    assert(firstCheck);
    firstCheck.points = 3;
    assert.notEqual(scoped(root, 'reference', 'mongodb', changedChecks).checksSha256,
      mongoReference.checksSha256);

    const changedReference = structuredClone(references.mongodb);
    changedReference.sourceSha256 = digest('f');
    const changedMongo = qualificationScopeIdentity({ kind: 'reference', release,
      stack: 'mongodb', reference: changedReference, stackBenchRoot: root });
    assert.notEqual(changedMongo.sha256, mongoReference.sha256);
    assert.deepEqual(scoped(root, 'reference', 'postgres'), postgresReference);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('stack-owned reset and version changes invalidate only their stack', () => {
  const root = fixture();
  try {
    const beforeMongoReference = scoped(root, 'reference', 'mongodb');
    const beforeMongoMutation = scoped(root, 'mutation', 'mongodb');
    const beforePostgresReference = scoped(root, 'reference', 'postgres');
    const beforePostgresMutation = scoped(root, 'mutation', 'postgres');
    const beforeNull = scoped(root, 'null');

    write(root, 'src/stacks/backends/postgres-operations.ts', 'changed postgres reset\n');
    assert.deepEqual(scoped(root, 'reference', 'mongodb'), beforeMongoReference);
    assert.deepEqual(scoped(root, 'mutation', 'mongodb'), beforeMongoMutation);
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256, beforePostgresReference.sha256);
    assert.notEqual(scoped(root, 'mutation', 'postgres').sha256, beforePostgresMutation.sha256);
    assert.deepEqual(scoped(root, 'null'), beforeNull);

    const afterResetMongo = scoped(root, 'reference', 'mongodb');
    const afterResetPostgres = scoped(root, 'reference', 'postgres');
    const afterResetNull = scoped(root, 'null');
    write(root, 'src/stacks/backends/postgres-identity.ts', 'changed postgres version\n');
    assert.deepEqual(scoped(root, 'reference', 'mongodb'), afterResetMongo);
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256, afterResetPostgres.sha256);
    assert.deepEqual(scoped(root, 'null'), afterResetNull);

    const afterVersionMongo = scoped(root, 'reference', 'mongodb');
    const afterVersionPostgres = scoped(root, 'reference', 'postgres');
    const afterVersionNull = scoped(root, 'null');
    write(root, 'src/stacks/backends/postgres-adapter.ts', 'changed postgres adapter\n');
    assert.deepEqual(scoped(root, 'reference', 'mongodb'), afterVersionMongo);
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256, afterVersionPostgres.sha256);
    assert.deepEqual(scoped(root, 'null'), afterVersionNull);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('shared grading changes invalidate every affected scope while mutation-only changes do not', () => {
  const root = fixture();
  try {
    const beforeReference = scoped(root, 'reference', 'mongodb');
    const beforeMutation = scoped(root, 'mutation', 'mongodb');
    write(root, 'grader/mutation-test.ts', 'changed mutation runner\n');
    assert.deepEqual(scoped(root, 'reference', 'mongodb'), beforeReference);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, beforeMutation.sha256);

    const beforeMongo = scoped(root, 'reference', 'mongodb');
    const beforeResetMutation = scoped(root, 'mutation', 'mongodb');
    const beforeNull = scoped(root, 'null');
    write(root, 'commands/reset-backend.ts', 'changed backend reset\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, beforeMongo.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, beforeResetMutation.sha256);
    assert.deepEqual(scoped(root, 'null'), beforeNull);

    const afterResetMongo = scoped(root, 'reference', 'mongodb');
    const afterResetPostgres = scoped(root, 'reference', 'postgres');
    const afterResetMutation = scoped(root, 'mutation', 'mongodb');
    write(root, 'grader/grade.ts', 'changed shared grader\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, afterResetMongo.sha256);
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256, afterResetPostgres.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, afterResetMutation.sha256);
    assert.notEqual(scoped(root, 'null').sha256, beforeNull.sha256);

    const afterGradeReference = scoped(root, 'reference', 'mongodb');
    const afterGradeMutation = scoped(root, 'mutation', 'mongodb');
    const afterGradeNull = scoped(root, 'null');
    write(root, 'commands/run-suite.ts', 'changed suite runner\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, afterGradeReference.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, afterGradeMutation.sha256);
    assert.deepEqual(scoped(root, 'null'), afterGradeNull);

    const beforeAdapterMongo = scoped(root, 'reference', 'mongodb');
    const beforeAdapterPostgres = scoped(root, 'reference', 'postgres');
    const beforeAdapterNull = scoped(root, 'null');
    write(root, 'src/stacks/stack-adapter-common.ts', 'changed shared adapter helpers\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, beforeAdapterMongo.sha256);
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256,
      beforeAdapterPostgres.sha256);
    assert.deepEqual(scoped(root, 'null'), beforeAdapterNull);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the spawned browser proxy changes every executable scope and is required', () => {
  const root = fixture();
  const helper = 'container/browser-network-proxy.ts';
  try {
    const before = (['reference', 'mutation', 'null'] as const)
      .map(kind => scoped(root, kind, kind === 'null' ? null : 'mongodb'));
    write(root, helper, 'changed browser proxy\n');
    for (const [index, kind] of (['reference', 'mutation', 'null'] as const).entries()) {
      assert.notEqual(scoped(root, kind, kind === 'null' ? null : 'mongodb').executableSha256,
        before[index]!.executableSha256);
    }
    rmSync(join(root, helper));
    for (const kind of ['reference', 'mutation', 'null'] as const) {
      assert.throws(() => scoped(root, kind, kind === 'null' ? null : 'mongodb'),
        /mapped input does not exist: container\/browser-network-proxy\.ts/);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference deployment and its container launcher invalidate qualification', () => {
  const root = fixture();
  try {
    const beforeReference = scoped(root, 'reference', 'mongodb');
    const beforeMutation = scoped(root, 'mutation', 'mongodb');
    const beforeNull = scoped(root, 'null');

    write(root, 'container/run-build.ts', 'changed coding container\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, beforeReference.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, beforeMutation.sha256);
    assert.deepEqual(scoped(root, 'null'), beforeNull);

    const afterLauncherReference = scoped(root, 'reference', 'mongodb');
    const afterLauncherMutation = scoped(root, 'mutation', 'mongodb');
    write(root, 'src/references/reference-agent.ts', 'changed reference agent\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, afterLauncherReference.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, afterLauncherMutation.sha256);
    assert.deepEqual(scoped(root, 'null'), beforeNull);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('unmapped executable imports and tampered identities fail closed', () => {
  const root = fixture();
  try {
    write(root, 'grader/grade.ts', 'await import(runtimeModule)\n');
    assert.throws(() => scoped(root, 'reference', 'mongodb'), /unmapped dynamic import/);
    write(root, 'grader/grade.ts', "import '../src/runtime/backend-control.js';\n");
    write(root, 'src/runtime/backend-control.ts',
      'import(workerData.module); const options = { workerData: { module: import.meta.url, spec, target } };\n');
    assert.doesNotThrow(() => scoped(root, 'reference', 'mongodb'));
    write(root, 'src/runtime/backend-control.ts',
      'import(workerData.module); const options = { workerData: { module: otherModule, spec, target } };\n');
    assert.throws(() => scoped(root, 'reference', 'mongodb'), /unmapped dynamic import/);
    write(root, 'grader/grade.ts', 'shared grader\n');
    const identity = scoped(root, 'reference', 'mongodb');
    assert.deepEqual(validateQualificationScopeIdentity(identity), identity);
    assert.throws(() => validateQualificationScopeIdentity({ ...identity, unknown: true }),
      /unknown/);
    assert.throws(() => validateQualificationScopeIdentity({ ...identity, sha256: digest('0') }),
      /does not match/);

    write(root, 'src/stacks/stack-adapters.ts',
      "import './backends/unowned-reset.js';\n");
    write(root, 'src/stacks/backends/unowned-reset.ts', 'unowned\n');
    assert.throws(() => scoped(root, 'reference', 'mongodb'), /names no registered stack/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('registering another stack changes no existing stack scope', () => {
  const root = fixture();
  const contracted = { ...release, task: { contracts: [{ path: 'contracts/cart.md' }] } };
  const cart = (http: string, convex = '') => write(root, 'tracks/ecommerce/contracts/cart.md',
    `Add items.\n\n<!-- interface:http -->\n${http}\n<!-- /interface -->\n`
    + (convex ? `\n<!-- interface:convex -->\n${convex}\n<!-- /interface -->\n` : ''));
  const all = () => ({
    postgres: scoped(root, 'reference', 'postgres', contracted),
    mongodb: scoped(root, 'mutation', 'mongodb', contracted),
    null: scoped(root, 'null', null, contracted),
  });
  const registry = (convex: boolean) => write(root, 'src/stacks/stack-adapters.ts', [
    "import { mongodbAdapter } from './backends/mongodb-adapter.js';",
    ...(convex ? ["import { convexAdapter } from './backends/convex-adapter.js';"] : []),
    "import { postgresAdapter } from './backends/postgres-adapter.js';",
    'const adapters = [',
    ...(convex ? ['  convexAdapter,'] : []),
    '  mongodbAdapter,',
    '  postgresAdapter,',
    '];',
    '',
  ].join('\n'));
  try {
    registry(false);
    cart('Use POST /api/cart.');
    const before = all();
    registry(true);
    write(root, 'src/stacks/backends/convex-adapter.ts', 'convex adapter\n');
    write(root, 'src/stacks/backends/convex/platform.sql', 'convex asset\n');
    cart('Use POST /api/cart.', 'Use api:add_to_cart.');
    assert.deepEqual(all(), before);

    cart('Use PUT /api/cart.', 'Use api:add_to_cart.');
    const afterHttp = all();
    assert.notEqual(afterHttp.postgres.sha256, before.postgres.sha256);
    assert.notEqual(afterHttp.mongodb.sha256, before.mongodb.sha256);
    assert.deepEqual(afterHttp.null, before.null);

    write(root, 'src/stacks/stack-adapters.ts', [
      "import { convexAdapter } from './backends/convex-adapter.js';",
      "import './backends/postgres-adapter.js';",
      'const fallback = convexAdapter.version;',
      '',
    ].join('\n'));
    assert.throws(() => scoped(root, 'reference', 'postgres', contracted),
      /uses another stack outside a registration/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a stack scope includes its runtime asset directory', () => {
  const root = fixture();
  try {
    write(root, 'src/stacks/backends/postgres/reset.sql', 'reset v1\n');
    const postgres = scoped(root, 'reference', 'postgres');
    const mongodb = scoped(root, 'reference', 'mongodb');
    write(root, 'src/stacks/backends/postgres/reset.sql', 'reset v2\n');
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256, postgres.sha256);
    assert.deepEqual(scoped(root, 'reference', 'mongodb'), mongodb);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a stack module that imports another stack module fails closed', () => {
  const root = fixture();
  try {
    write(root, 'src/stacks/backends/postgres-operations.ts', "import './mongodb-operations.js';\n");
    assert.throws(() => scoped(root, 'reference', 'postgres'), /imports a module owned by mongodb/);
    write(root, 'src/stacks/postgres-sql.ts', 'shared sql\n');
    write(root, 'src/stacks/backends/postgres-operations.ts', "import '../postgres-sql.js';\n");
    const postgres = scoped(root, 'reference', 'postgres');
    write(root, 'src/stacks/postgres-sql.ts', 'changed shared sql\n');
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256, postgres.sha256);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
