import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { qualificationScopeIdentity, validateQualificationScopeIdentity }
  from '../src/composition/qualification-scope.mjs';

const digest = character => character.repeat(64);
const release = {
  id: 'ecommerce.l1', version: '1.0.0', contentSha256: digest('a'), track: 'ecommerce',
  checkCatalog: [
    { stableKey: 'check.a', executionId: 'suite', source: 'scenarios/a.json',
      featureId: '1', criterionId: 'a', points: 1 },
    { stableKey: 'check.b', executionId: 'suite', source: 'scenarios/b.json',
      featureId: '1', criterionId: 'b', points: 2 },
  ],
};
const references = {
  mongodb: { backend: 'mongodb', id: 'mongo-reference', sourceSha256: digest('b') },
  postgres: { backend: 'postgres', id: 'postgres-reference', sourceSha256: digest('c') },
};
const mutations = {
  mongodb: { backend: 'mongodb', executionSha256: digest('d') },
  postgres: { backend: 'postgres', executionSha256: digest('e') },
};

function write(root, path, source = '') {
  const target = join(root, path);
  mkdirSync(dirname(target), { recursive: true });
  writeFileSync(target, source);
}

function fixture() {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-qualification-scope-'));
  for (const path of [
    'commands/run-suite.mjs', 'commands/check-actions.mjs', 'commands/reset-backend.mjs',
    'commands/bench.mjs',
    'commands/null-control.mjs', 'src/references/reference-live.mjs', 'grader/grade.mjs',
    'grader/mutation-test.mjs', 'linter/lint.mjs', 'package.json', 'package-lock.json',
    'docker-compose.yaml', 'appliance/Controller.Dockerfile', 'appliance/docker-compose.yaml',
    'tracks/ecommerce/walk.mjs',
  ]) write(root, path, `${path}\n`);
  return root;
}

function scoped(root, kind, stack = null, changedRelease = release) {
  return qualificationScopeIdentity({
    kind,
    release: changedRelease,
    stack,
    reference: stack === null ? null : references[stack],
    mutation: kind === 'mutation' ? mutations[stack] : null,
    stackBenchRoot: root,
  });
}

test('qualification identities isolate stack, mutation, and selected-check inputs', () => {
  const root = fixture();
  try {
    const mongoReference = scoped(root, 'reference', 'mongodb');
    const postgresReference = scoped(root, 'reference', 'postgres');
    const mongoMutation = scoped(root, 'mutation', 'mongodb');
    assert.equal(mongoReference.executableSha256, postgresReference.executableSha256);
    assert.equal(mongoReference.checksSha256, postgresReference.checksSha256);
    assert.notDeepEqual(mongoReference.stack, postgresReference.stack);
    assert.notEqual(mongoReference.sha256, postgresReference.sha256);
    assert.notEqual(mongoReference.executableSha256, mongoMutation.executableSha256);

    const changedChecks = structuredClone(release);
    changedChecks.checkCatalog[0].points = 3;
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

test('shared grading changes invalidate every affected scope while mutation-only changes do not', () => {
  const root = fixture();
  try {
    const beforeReference = scoped(root, 'reference', 'mongodb');
    const beforeMutation = scoped(root, 'mutation', 'mongodb');
    write(root, 'grader/mutation-test.mjs', 'changed mutation runner\n');
    assert.deepEqual(scoped(root, 'reference', 'mongodb'), beforeReference);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, beforeMutation.sha256);

    const beforeMongo = scoped(root, 'reference', 'mongodb');
    const beforePostgres = scoped(root, 'reference', 'postgres');
    const beforeResetMutation = scoped(root, 'mutation', 'mongodb');
    const beforeNull = scoped(root, 'null');
    write(root, 'commands/reset-backend.mjs', 'changed backend reset\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, beforeMongo.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, beforeResetMutation.sha256);
    assert.deepEqual(scoped(root, 'null'), beforeNull);

    const afterResetMongo = scoped(root, 'reference', 'mongodb');
    const afterResetPostgres = scoped(root, 'reference', 'postgres');
    const afterResetMutation = scoped(root, 'mutation', 'mongodb');
    write(root, 'grader/grade.mjs', 'changed shared grader\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, afterResetMongo.sha256);
    assert.notEqual(scoped(root, 'reference', 'postgres').sha256, afterResetPostgres.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, afterResetMutation.sha256);
    assert.notEqual(scoped(root, 'null').sha256, beforeNull.sha256);

    const afterGradeReference = scoped(root, 'reference', 'mongodb');
    const afterGradeMutation = scoped(root, 'mutation', 'mongodb');
    const afterGradeNull = scoped(root, 'null');
    write(root, 'commands/run-suite.mjs', 'changed suite runner\n');
    assert.notEqual(scoped(root, 'reference', 'mongodb').sha256, afterGradeReference.sha256);
    assert.notEqual(scoped(root, 'mutation', 'mongodb').sha256, afterGradeMutation.sha256);
    assert.deepEqual(scoped(root, 'null'), afterGradeNull);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('unmapped executable imports and tampered identities fail closed', () => {
  const root = fixture();
  try {
    write(root, 'grader/grade.mjs', 'await import(runtimeModule)\n');
    assert.throws(() => scoped(root, 'reference', 'mongodb'), /unmapped dynamic import/);
    write(root, 'grader/grade.mjs', 'shared grader\n');
    const identity = scoped(root, 'reference', 'mongodb');
    assert.deepEqual(validateQualificationScopeIdentity(identity), identity);
    assert.throws(() => validateQualificationScopeIdentity({ ...identity, unknown: true }),
      /unknown/);
    assert.throws(() => validateQualificationScopeIdentity({ ...identity, sha256: digest('0') }),
      /does not match/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
