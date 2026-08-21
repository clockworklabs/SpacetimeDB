import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { generateSpdxImageSbom, materializeReleaseManifest } from '../src/releases/release-bundle.mjs';
import { RELEASE_MANIFEST_SCHEMA_VERSION } from '../src/releases/release-manifest.mjs';

const digest = 'a'.repeat(64);
const reference = `registry.example/controller@sha256:${digest}`;

function document(boundDigest = digest) {
  return { spdxVersion: 'SPDX-2.3', dataLicense: 'CC0-1.0', SPDXID: 'SPDXRef-DOCUMENT',
    creationInfo: { creators: ['Tool: test'] }, packages: [{ externalRefs: [{
      referenceLocator: `pkg:oci/controller@sha256:${boundDigest}` }] }] };
}

test('SBOM generation uses registry resolution, verifies digest binding, and refuses overwrite', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-sbom-'));
  try {
    const outputPath = join(root, 'controller.spdx.json');
    const calls = [];
    const result = generateSpdxImageSbom({ reference, outputPath,
      runCommand: (executable, args) => {
        calls.push({ executable, args });
        writeFileSync(args[args.indexOf('--output') + 1], JSON.stringify(document()));
      } });
    assert.equal(result.image, reference);
    assert.equal(result.bytes, readFileSync(outputPath).length);
    assert.equal(calls[0].executable, 'docker');
    assert.equal(calls[0].args.at(-1), `registry://${reference}`);
    assert.ok(calls[0].args.includes('linux/amd64'));
    assert.throws(() => generateSpdxImageSbom({ reference, outputPath, runCommand: () => {} }),
      /refusing to overwrite/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('SBOM generation leaves no output after invalid or unbound tool output', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-sbom-'));
  try {
    const outputPath = join(root, 'controller.spdx.json');
    assert.throws(() => generateSpdxImageSbom({ reference, outputPath,
      runCommand: (_executable, args) => writeFileSync(args[args.indexOf('--output') + 1],
        JSON.stringify(document('b'.repeat(64)))) }), /does not bind image digest/);
    assert.equal(existsSync(outputPath), false);
    assert.throws(() => generateSpdxImageSbom({ reference: `https://${reference}`,
      outputPath, runCommand: () => {} }), /normalized registry reference/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('release bundle CLI rejects unknown and duplicate options', () => {
  const script = join(import.meta.dirname, '..', 'src', 'releases', 'release-bundle.mjs');
  const unknown = spawnSync(process.execPath, [script, 'sbom', reference,
    '--output', 'x', '--surprise', 'y'], { encoding: 'utf8' });
  assert.equal(unknown.status, 2);
  assert.match(unknown.stderr, /unknown option --surprise/);
  const duplicate = spawnSync(process.execPath, [script, 'sbom', reference,
    '--output', 'x', '--output', 'y'], { encoding: 'utf8' });
  assert.equal(duplicate.status, 2);
  assert.match(duplicate.stderr, /duplicate option --output/);
});

test('manifest assembly computes file metadata and rejects missing and escaping inputs', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-bundle-'));
  try {
    const roles = ['controller', 'build-sandbox', 'postgres', 'mongodb'];
    const files = [['compose.yaml', 'compose'], ['deps', 'dependency'], ['OPERATOR.md', 'operator-guide'],
      ['secrets.example', 'secrets-template'], ['SUPPORT.md', 'support-policy'],
      ...roles.map(role => [`sbom/${role}.json`, 'sbom'])]
      .map(([path, role]) => ({ path, role }));
    for (const { path } of files) {
      const absolute = join(root, ...path.split('/'));
      mkdirSync(dirname(absolute), { recursive: true });
      writeFileSync(absolute, path.startsWith('sbom/') ? JSON.stringify(document()) : path);
    }
    const specification = { schemaVersion: RELEASE_MANIFEST_SCHEMA_VERSION,
      id: 'stack-bench', version: '1.0.0', state: 'candidate', sourceRevision: 'a'.repeat(40),
      sourceSha256: 'b'.repeat(64), supportedRunner: { os: 'linux', architecture: 'amd64',
        stateRoot: '/var/lib/stack-bench', networkMode: 'host', dockerSocket: true },
      images: roles.map(role => ({ id: `stack-bench-${role}`, role, reference,
        digest, platform: 'linux/amd64', sbomPath: `sbom/${role}.json` })), files,
      outboundDestinations: [], secrets: [], signing: null };
    const manifest = materializeReleaseManifest(specification, root);
    assert.equal(manifest.files.length, files.length);
    assert.ok(manifest.files.every(file => file.sha256.length === 64 && file.bytes > 0));

    assert.throws(() => materializeReleaseManifest({ ...specification,
      files: [...files, { path: 'missing', role: 'support-policy', extra: true }] }, root),
    /accepts only path and role/);
    assert.throws(() => materializeReleaseManifest({ ...specification,
      files: [...files, { path: '../escape', role: 'support-policy' }] }, root), /invalid bundle path/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
