import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { hashDirectory } from '../src/evidence/provenance.js';
import { loadReferenceRegistry, inspectImportedReference, selectReferenceFixture,
  validateReferenceRegistry, type ReferenceFixture, type ReferenceRegistry }
  from '../src/references/reference-fixtures.js';
import { resolveReferenceSelection } from '../src/references/reference-selection.js';

test('the reference registry binds its current statuses and provenance', () => {
  const registry = loadReferenceRegistry();
  const result = validateReferenceRegistry(registry);
  assert.deepEqual(result.issues, []);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'active').length, 0);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'candidate').length, 3);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'blocked').length, 0);
  const escaped = structuredClone(registry);
  const escapedFixture = escaped.fixtures[0];
  assert(escapedFixture, 'the registry must contain a fixture');
  escapedFixture.origin = { kind: 'imported' };
  assert(validateReferenceRegistry(escaped).issues.some(issue => issue.includes('must be authored')));
  escapedFixture.source = { basePath: 'reference-apps/old', patchPath: 'reference-apps/old.json' };
  assert(validateReferenceRegistry(escaped).issues.some(issue =>
    issue.includes('source overlays are not supported')));
});

test('a recipe-bound full fixture can serve only its declared progression action levels', () => {
  const registry = loadReferenceRegistry();
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    for (const level of [1, 2, 3, 4, 5, 6]) {
      assert.equal(selectReferenceFixture(registry, { backend, track: 'ecommerce', level,
        recipe: 'ecommerce.progression-catalog@2.0.1' }).id,
      `ecommerce-reference-${backend}`);
    }
  }

  const invalid = structuredClone(registry);
  const invalidFixture = invalid.fixtures[0];
  assert(invalidFixture, 'the registry must contain a fixture');
  invalidFixture.actionLevels = [1, 2, 2, 6];
  const issues = validateReferenceRegistry(invalid).issues.filter(issue =>
    issue.startsWith('ecommerce-reference-mongodb:'));
  assert(issues.some(issue => issue.includes('unique positive integer levels')));
  const invalidRange = structuredClone(registry);
  const invalidRangeFixture = invalidRange.fixtures[0];
  assert(invalidRangeFixture, 'the registry must contain a fixture');
  invalidRangeFixture.actionLevels = [1, 5, 7];
  assert(validateReferenceRegistry(invalidRange).issues.some(issue =>
    issue.includes('cannot exceed the fixture level')));
});

test('reference selection uses only current recipe fixtures', () => {
  const registry = loadReferenceRegistry();
  assert.equal(selectReferenceFixture(registry, { backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.sequential-l1@2.5.0' }).id, 'ecommerce-reference-mongodb');
  const blocked = structuredClone(registry);
  const blockedFixture = blocked.fixtures
    .find(fixture => fixture.id === 'ecommerce-reference-mongodb');
  assert(blockedFixture, 'the current L1 fixture must exist');
  blockedFixture.status = 'blocked';
  assert.throws(() => selectReferenceFixture(blocked, { backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.sequential-l1@2.5.0' }), /exactly one/);
});

test('default reference tooling follows the current candidate recipe', () => {
  const registry = loadReferenceRegistry();
  const selection = resolveReferenceSelection(registry, {
    backend: 'mongodb', track: 'ecommerce', level: 1,
  });
  assert.equal(selection.recipe, 'ecommerce.sequential-l1@2.5.0');
  assert.equal(selection.binding.status, 'candidate');
  assert.equal(selection.fixture.id, 'ecommerce-reference-mongodb');

  const l2 = resolveReferenceSelection(registry, {
    backend: 'mongodb', track: 'ecommerce', level: 2,
  });
  assert.equal(l2.recipe, 'ecommerce.sequential-l2@1.6.0');
  assert.equal(l2.fixture.id, 'ecommerce-reference-mongodb');

  assert.throws(() => resolveReferenceSelection(registry, {
    backend: 'mongodb', track: 'ecommerce', level: 1,
    recipe: 'ecommerce.sequential-l1@0.0.0',
  }), /requires exactly one catalogued ecommerce\.sequential-l1@0\.0\.0; found 0/);
});

test('the L2 recipe selects one current fixture per backend', () => {
  const registry = loadReferenceRegistry();
  for (const backend of ['mongodb', 'postgres', 'spacetime']) {
    assert.equal(selectReferenceFixture(registry, { backend, track: 'ecommerce', level: 2,
      recipe: 'ecommerce.sequential-l2@1.6.0' }).id,
    `ecommerce-reference-${backend}`);
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
    const fixture: ReferenceFixture & { imported: { path: string; sourceSha256: string } } = {
      id: 'linked', backend: 'mongodb', track: 'ecommerce',
      level: 1, status: 'candidate', targetPath: 'reference-apps/linked', imported: {
      path: 'reference-apps/linked', sourceSha256: hashDirectory(target).sha256,
    } };
    const link = join(target, 'unchecked-link.txt');
    try { symlinkSync(join(target, 'server', 'package.json'), link, 'file'); }
    catch (error) {
      if (isFileSystemError(error) && ['EPERM', 'EACCES'].includes(error.code)) {
        t.skip('filesystem cannot create test symlinks'); return;
      }
      throw error;
    }
    assert.equal(hashDirectory(target).sha256, fixture.imported.sourceSha256);
    const failure = inspectImportedReference(fixture, { root }).failures[0];
    assert(failure, 'the symlink must produce an inspection failure');
    assert.match(failure, /unsupported filesystem entry/);
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
    const fixture: ReferenceFixture & { imported: { path: string; sourceSha256: string } } = {
      id: 'import', backend: 'mongodb', track: 'ecommerce',
      level: 1, status: 'candidate', targetPath: 'reference-apps/example', imported: {
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

test('authored references bind checked-in bytes', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-authored-reference-'));
  try {
    const target = join(root, 'reference-apps', 'authored');
    mkdirSync(join(target, 'server'), { recursive: true });
    writeFileSync(join(target, 'server', 'package.json'), '{}\n');
    writeFileSync(join(target, 'server', 'package-lock.json'), '{"lockfileVersion":3}\n');
    writeFileSync(join(target, 'reference.json'), JSON.stringify({
      schemaVersion: 1, kind: 'node-api', installDirectories: ['server'],
    }));
    const fixture: ReferenceFixture & { imported: { path: string; sourceSha256: string } } = {
      id: 'authored', backend: 'mongodb', track: 'ecommerce', level: 2,
      status: 'candidate', targetPath: 'reference-apps/authored', mutationManifests: [],
      origin: { kind: 'authored', note: 'Maintained as a benchmark oracle.' },
      imported: { path: 'reference-apps/authored', sourceSha256: hashDirectory(target).sha256 } };
    const registry: ReferenceRegistry = { schemaVersion: 4, fixtures: [fixture] };

    assert.deepEqual(validateReferenceRegistry(registry, { root }).issues, []);
    const reused = structuredClone(fixture);
    reused.id = 'authored-next-level';
    reused.level = 3;
    registry.fixtures.push(reused);
    assert.deepEqual(validateReferenceRegistry(registry, { root }).issues, []);
    assert.equal(inspectImportedReference(fixture, { root }).ok, true);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

function isFileSystemError(error: unknown): error is NodeJS.ErrnoException & { code: string } {
  return error instanceof Error && 'code' in error && typeof error.code === 'string';
}
