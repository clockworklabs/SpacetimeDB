import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { sha256 } from '../provenance.mjs';
import { RELEASE_MANIFEST_SCHEMA_VERSION, validateReleaseManifest,
  verifyReleaseBundle } from '../release-manifest.mjs';

const roles = ['controller', 'build-sandbox', 'postgres', 'mongodb'];

function spdx(role, digest) {
  return JSON.stringify({ spdxVersion: 'SPDX-2.3', dataLicense: 'CC0-1.0',
    SPDXID: 'SPDXRef-DOCUMENT', name: `stack-bench-${role}`,
    creationInfo: { creators: ['Tool: fixture'], created: '2026-08-12T00:00:00Z' },
    packages: [{ name: role, SPDXID: 'SPDXRef-DocumentRoot', externalRefs: [{
      referenceLocator: `pkg:oci/${role}@sha256:${digest}` }]}] }, null, 2);
}

function fixture(root, { state = 'candidate' } = {}) {
  const digests = Object.fromEntries(roles.map((role, index) => [role, String(index + 1).repeat(64)]));
  const specifications = [
    ['compose.yaml', 'compose', 'compose\n'],
    ['deps.tar.zst', 'dependency', 'dependency\n'],
    ['OPERATOR.md', 'operator-guide', 'operator\n'],
    ['secrets.example', 'secrets-template', 'secrets\n'],
    ['SUPPORT.md', 'support-policy', 'support\n'],
    ...roles.map(role => [`sbom/${role}.spdx.json`, 'sbom', spdx(role, digests[role])]),
    ...(state === 'qualified' ? [
      ['signing/cosign.pub', 'public-key', 'trusted public key\n'],
    ] : []),
  ];
  const files = specifications.map(([path, role, content]) => {
    const absolute = join(root, ...path.split('/'));
    mkdirSync(dirname(absolute), { recursive: true });
    writeFileSync(absolute, content);
    const bytes = readFileSync(absolute);
    return { path, role, sha256: sha256(bytes), bytes: statSync(absolute).size };
  });
  if (state === 'qualified') {
    mkdirSync(join(root, 'signing'), { recursive: true });
    writeFileSync(join(root, 'signing', 'release-manifest.sigstore.json'), '{}\n');
  }
  const images = roles.map(role => ({ id: `stack-bench-${role}`, role,
    reference: `registry.example/stack-bench/${role}@sha256:${digests[role]}`,
    digest: digests[role], platform: 'linux/amd64', sbomPath: `sbom/${role}.spdx.json` }));
  return {
    schemaVersion: RELEASE_MANIFEST_SCHEMA_VERSION,
    id: 'stack-bench-v1', version: '1.0.0', state,
    sourceRevision: 'a'.repeat(40), sourceSha256: 'b'.repeat(64),
    supportedRunner: { os: 'linux', architecture: 'amd64',
      stateRoot: '/var/lib/stack-bench', networkMode: 'host', dockerSocket: true },
    images, files,
    outboundDestinations: [
      { owner: 'build-sandbox', url: 'https://registry.npmjs.org' },
      { owner: 'claude-code', url: 'https://api.anthropic.com' },
    ],
    secrets: [{ id: 'provider-api-key', adapter: 'claude-code',
      composeTarget: '/run/secrets/provider_api_key', required: true }],
    signing: state === 'qualified' ? { scheme: 'cosign-public-key-v1',
      publicKeyPath: 'signing/cosign.pub',
      manifestBundlePath: 'signing/release-manifest.sigstore.json' } : null,
  };
}

test('candidate manifest binds files and digest-bearing SPDX without placeholder signatures', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  try {
    const manifest = fixture(root);
    assert.deepEqual(validateReleaseManifest(manifest), manifest);
    const verified = verifyReleaseBundle(manifest, root);
    assert.equal(verified.ok, true, JSON.stringify(verified.results));
    assert.equal(verified.verificationLevel, 'candidate-file-integrity');
    assert.equal(verified.results.filter(result => result.check === 'spdx-image-binding').length, 4);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('release verification detects changed, missing, escaping, and wrong-image SBOM files', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  try {
    const changed = fixture(root);
    writeFileSync(join(root, 'compose.yaml'), 'changed');
    assert.equal(verifyReleaseBundle(changed, root).results
      .find(result => result.path === 'compose.yaml').reason, 'size mismatch');

    const missing = fixture(root);
    rmSync(join(root, 'OPERATOR.md'));
    assert.equal(verifyReleaseBundle(missing, root).results
      .find(result => result.path === 'OPERATOR.md').reason, 'missing');

    const escaped = fixture(root);
    escaped.files[0].path = '../compose.yaml';
    assert.throws(() => verifyReleaseBundle(escaped, root), /relative POSIX path/);

    const wrongSbom = fixture(root);
    const sbomFile = wrongSbom.files.find(file => file.path === wrongSbom.images[0].sbomPath);
    const sbomPath = join(root, ...sbomFile.path.split('/'));
    const document = JSON.parse(readFileSync(sbomPath, 'utf8'));
    document.packages[0].externalRefs[0].referenceLocator = `pkg:oci/controller@sha256:${'f'.repeat(64)}`;
    writeFileSync(sbomPath, JSON.stringify(document));
    sbomFile.sha256 = sha256(readFileSync(sbomPath));
    sbomFile.bytes = statSync(sbomPath).size;
    assert.match(verifyReleaseBundle(wrongSbom, root).results
      .find(result => result.check === 'spdx-image-binding' && !result.ok).reason,
    /does not bind image digest/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('release schema refuses mutable images, incomplete roles, false signing, and unsafe inputs', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  try {
    const mutable = fixture(root);
    mutable.images[0].reference = 'registry.example/controller:latest';
    assert.throws(() => validateReleaseManifest(mutable), /exact digest/);

    const malformed = fixture(root);
    malformed.images[0].reference = `https://registry.example/controller@sha256:${malformed.images[0].digest}`;
    assert.throws(() => validateReleaseManifest(malformed), /normalized registry reference/);

    const incomplete = fixture(root);
    incomplete.images.pop();
    assert.throws(() => validateReleaseManifest(incomplete), /missing mongodb/);

    const insecure = fixture(root);
    insecure.outboundDestinations[0].url = 'http://registry.npmjs.org';
    assert.throws(() => validateReleaseManifest(insecure), /must be HTTPS/);

    const secretEscape = fixture(root);
    secretEscape.secrets[0].composeTarget = '/run/secrets/../host';
    assert.throws(() => validateReleaseManifest(secretEscape), /under \/run\/secrets/);

    const unknownSignature = fixture(root);
    unknownSignature.images[0].signaturePath = 'signatures/fake';
    assert.throws(() => validateReleaseManifest(unknownSignature), /signaturePath is unknown/);

    const qualified = fixture(root, { state: 'qualified' });
    qualified.signing = null;
    assert.throws(() => validateReleaseManifest(qualified), /must be an object/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('qualified verification requires the signed disk manifest and an external matching trust key', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  try {
    const manifest = fixture(root, { state: 'qualified' });
    const manifestPath = join(root, 'release.json');
    const trustedKeyPath = join(root, '..', `trusted-${Date.now()}.pub`);
    writeFileSync(manifestPath, `${JSON.stringify(manifest, null, 2)}\n`);
    writeFileSync(trustedKeyPath, 'trusted public key\n');
    try {
      assert.throws(() => verifyReleaseBundle(manifest, root, { manifestPath }),
        /external trusted public key/);
      writeFileSync(trustedKeyPath, 'wrong key\n');
      assert.throws(() => verifyReleaseBundle(manifest, root, { manifestPath, trustedKeyPath }),
        /differs from the external trusted/);
      writeFileSync(trustedKeyPath, 'trusted public key\n');
      const calls = [];
      const verified = verifyReleaseBundle(manifest, root, { manifestPath, trustedKeyPath,
        cosignPath: 'cosign-test', runCommand: (executable, args) => {
          calls.push({ executable, args });
          return { ok: true };
        } });
      assert.equal(verified.ok, true);
      assert.equal(verified.verificationLevel, 'qualified-cryptographic');
      assert.equal(calls.length, 5);
      assert.deepEqual(calls.map(call => call.args[0]), ['verify-blob', 'verify', 'verify', 'verify', 'verify']);
      assert.ok(calls.every(call => call.executable === 'cosign-test'));

      const failed = verifyReleaseBundle(manifest, root, { manifestPath, trustedKeyPath,
        runCommand: (_executable, args) => ({ ok: args[0] !== 'verify', detail: 'bad bundle' }) });
      assert.equal(failed.ok, false);
      assert.equal(failed.cryptographicVerification.checks[0].detail, 'bad bundle');
    } finally { rmSync(trustedKeyPath, { force: true }); }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
