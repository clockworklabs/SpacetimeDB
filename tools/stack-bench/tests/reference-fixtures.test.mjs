import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { hashDirectory } from '../provenance.mjs';
import { inspectReferenceCandidate, loadReferenceRegistry,
  inspectImportedReference, validateReferenceRegistry } from '../reference-fixtures.mjs';

test('the reference registry binds candidate, blocked and legacy manifest lifecycles', () => {
  const registry = loadReferenceRegistry();
  const result = validateReferenceRegistry(registry);
  assert.deepEqual(result.issues, []);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'active').length, 0);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'candidate').length, 3);
  assert.equal(registry.fixtures.filter(fixture => fixture.status === 'blocked').length, 3);
  const escaped = structuredClone(registry);
  escaped.fixtures[0].archivedEvidence = ['results/unbound-grade.json'];
  escaped.fixtures[0].origin.source = '../outside';
  assert(validateReferenceRegistry(escaped).issues.some(issue => issue.includes('must stay inside')));
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
    assert.deepEqual(inspectReferenceCandidate(fixture, { root }), {
      id: 'candidate', available: true, ok: true, sourceSha256: fixture.origin.sourceSha256,
      sourceFiles: 1, archivedEvidence: 1, failures: [],
    });
    writeFileSync(join(source, 'app.txt'), 'changed\n');
    assert.equal(inspectReferenceCandidate(fixture, { root }).ok, false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
