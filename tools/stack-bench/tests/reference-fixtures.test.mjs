import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { hashDirectory } from '../src/evidence/provenance.mjs';
import { inspectReferenceCandidate, loadReferenceRegistry,
  inspectImportedReference, selectReferenceFixture,
  validateReferenceRegistry } from '../src/references/reference-fixtures.mjs';
import { resolveReferenceSelection } from '../src/references/reference-selection.mjs';

test('the reference registry binds active, blocked, and historical provenance lifecycles', () => {
  const registry = loadReferenceRegistry();
  const result = validateReferenceRegistry(registry);
  assert.deepEqual(result.issues, []);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'active').length, 12);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'candidate').length, 3);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'blocked').length, 3);
  const escaped = structuredClone(registry);
  escaped.fixtures[0].archivedEvidence = ['results/unbound-grade.json'];
  escaped.fixtures[0].origin.source = '../outside';
  assert(validateReferenceRegistry(escaped).issues.some(issue => issue.includes('must stay inside')));
});

test('reference selection uses an exact recipe release and otherwise keeps the unscoped active fixture', () => {
  const registry = loadReferenceRegistry();
  assert.equal(selectReferenceFixture(registry, { backend: 'mongodb', track: 'ecommerce', level: 1 }).id,
    'ecommerce-l1-mongodb');
  assert.equal(selectReferenceFixture(registry, { backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.l1-modular@2.3.0' }).id, 'ecommerce-l1-direct-actions-mongodb');
  assert.equal(selectReferenceFixture(registry, { backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.l1-modular@2.4.0' }).id, 'ecommerce-l1-action-inputs-2.4-mongodb');
  assert.equal(selectReferenceFixture(registry, { backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.l1-standard@1.1.0' }).id, 'ecommerce-l1-mongodb');
  const blocked = structuredClone(registry);
  blocked.fixtures.find(fixture => fixture.id === 'ecommerce-l1-direct-actions-mongodb').status = 'blocked';
  assert.throws(() => selectReferenceFixture(blocked, { backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.l1-modular@2.3.0' }), /exactly one/);
});

test('default reference tooling follows the promoted recipe instead of an unscoped legacy fixture', () => {
  const registry = loadReferenceRegistry();
  const promoted = resolveReferenceSelection(registry, {
    backend: 'mongodb', track: 'ecommerce', level: 1,
  });
  assert.equal(promoted.recipe, 'ecommerce.l1-modular@2.3.0');
  assert.equal(promoted.binding.status, 'promoted');
  assert.equal(promoted.fixture.id, 'ecommerce-l1-direct-actions-mongodb');

  const promotedL2 = resolveReferenceSelection(registry, {
    backend: 'mongodb', track: 'ecommerce', level: 2,
  });
  assert.equal(promotedL2.recipe, 'ecommerce.l2-standard@1.4.0');
  assert.equal(promotedL2.binding.status, 'promoted');
  assert.equal(promotedL2.fixture.id, 'ecommerce-l2-server-actions-mongodb');

  assert.throws(() => resolveReferenceSelection(registry, {
    backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.l1-standard@1.1.0',
  }), /no recipe release|retired|requires exactly one catalogued/);
});

test('the promoted L2 recipe selects one derived fixture per backend without replacing legacy fixtures', () => {
  const registry = loadReferenceRegistry();
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    assert.equal(selectReferenceFixture(registry, { backend, track: 'ecommerce', level: 2 }).id,
      `ecommerce-l2-${backend}`);
    assert.equal(selectReferenceFixture(registry, { backend, track: 'ecommerce', level: 2,
      recipe: 'ecommerce.l2-standard@1.4.0' }).id,
    `ecommerce-l2-server-actions-${backend}`);
    assert.equal(selectReferenceFixture(registry, { backend, track: 'ecommerce', level: 2,
      recipe: 'ecommerce.l2-standard@1.2.0' }).id,
    `ecommerce-l2-${backend}`);
  }
});

test('reference inspection rejects a symlink that the regular-file hash does not bind', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-link-'));
  try {
    const target = join(root, 'reference-apps', 'linked');
    mkdirSync(join(target, 'server'), { recursive: true });
    writeFileSync(join(target, 'server', 'package.json'), '{}\n');
    writeFileSync(join(target, 'server', 'package-lock.json'), '{"lockfileVersion":3}\n');
    writeFileSync(join(target, 'reference.json'), JSON.stringify({
      schemaVersion: 1, kind: 'node-api', installDirectories: ['server'],
    }));
    const fixture = { id: 'linked', targetPath: 'reference-apps/linked', imported: {
      path: 'reference-apps/linked', sourceSha256: hashDirectory(target).sha256,
    } };
    const link = join(target, 'unchecked-link.txt');
    try { symlinkSync(join(target, 'server', 'package.json'), link, 'file'); }
    catch (error) {
      if (['EPERM', 'EACCES'].includes(error.code)) { t.skip('filesystem cannot create test symlinks'); return; }
      throw error;
    }
    assert.equal(hashDirectory(target).sha256, fixture.imported.sourceSha256);
    assert.match(inspectImportedReference(fixture, { root }).failures[0], /unsupported filesystem entry/);
    unlinkSync(link);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('imported fixture inspection requires locks and rejects local or generated files', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-import-'));
  try {
    const target = join(root, 'reference-apps', 'example');
    mkdirSync(join(target, 'server'), { recursive: true });
    writeFileSync(join(target, 'server', 'package.json'), '{}\n');
    writeFileSync(join(target, 'server', 'package-lock.json'), '{"lockfileVersion":3}\n');
    writeFileSync(join(target, 'reference.json'), JSON.stringify({ schemaVersion: 1, kind: 'node-api', installDirectories: ['server'] }));
    const fixture = { id: 'import', targetPath: 'reference-apps/example', imported: {
      path: 'reference-apps/example', sourceSha256: hashDirectory(target).sha256,
    } };
    assert.equal(inspectImportedReference(fixture, { root }).ok, true);

    mkdirSync(join(target, 'server', 'dist'));
    writeFileSync(join(target, 'server', 'dist', 'app.js'), 'const local = "D:/Development/private";\n');
    fixture.imported.sourceSha256 = hashDirectory(target).sha256;
    const result = inspectImportedReference(fixture, { root });
    assert.equal(result.ok, false);
    assert(result.failures.some(failure => failure.includes('generated directory')));
    assert(result.failures.some(failure => failure.includes('workstation absolute path')));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('candidate inspection binds exact source bytes and treats archived evidence as opaque', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-'));
  try {
    const source = join(root, 'candidate');
    mkdirSync(source);
    writeFileSync(join(source, 'app.txt'), 'known-good\n');
    const evidencePath = join(root, 'grade.json');
    writeFileSync(evidencePath, 'pre-v1 bytes that the active harness must not parse\n');
    const fixture = { id: 'candidate', origin: { source: 'candidate', sourceSha256: hashDirectory(source).sha256 },
      archivedEvidence: ['grade.json'] };
    fixture.origin.kind = 'historical-import';
    assert.deepEqual(inspectReferenceCandidate(fixture, { root }), {
      id: 'candidate', origin: 'historical-import', available: true, ok: true, sourceSha256: fixture.origin.sourceSha256,
      sourceFiles: 1, archivedEvidence: 1, failures: [],
    });
    writeFileSync(join(source, 'app.txt'), 'changed\n');
    assert.equal(inspectReferenceCandidate(fixture, { root }).ok, false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('authored references bind checked-in bytes without inventing historical evidence', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-authored-reference-'));
  try {
    const target = join(root, 'reference-apps', 'authored');
    mkdirSync(join(target, 'server'), { recursive: true });
    writeFileSync(join(target, 'server', 'package.json'), '{}\n');
    writeFileSync(join(target, 'server', 'package-lock.json'), '{"lockfileVersion":3}\n');
    writeFileSync(join(target, 'reference.json'), JSON.stringify({
      schemaVersion: 1, kind: 'node-api', installDirectories: ['server'],
    }));
    const fixture = { id: 'authored', backend: 'mongodb', track: 'ecommerce', level: 2,
      status: 'candidate', targetPath: 'reference-apps/authored', mutationManifests: [],
      origin: { kind: 'authored', note: 'Maintained as a benchmark oracle.' },
      imported: { path: 'reference-apps/authored', sourceSha256: hashDirectory(target).sha256 } };
    const registry = { schemaVersion: 4, fixtures: [fixture] };

    assert.deepEqual(validateReferenceRegistry(registry, { root }).issues, []);
    const reused = structuredClone(fixture);
    reused.id = 'authored-next-level';
    reused.level = 3;
    registry.fixtures.push(reused);
    assert.deepEqual(validateReferenceRegistry(registry, { root }).issues, []);
    assert.deepEqual(inspectReferenceCandidate(fixture, { root }), {
      id: 'authored', origin: 'authored', available: true, ok: true,
      sourceSha256: fixture.imported.sourceSha256, sourceFiles: 3,
      archivedEvidence: 0, failures: [],
    });

    fixture.archivedEvidence = ['archive/pre-v1/results/fake.json'];
    assert(validateReferenceRegistry(registry, { root }).issues
      .some(issue => issue.includes('cannot claim archivedEvidence')));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
