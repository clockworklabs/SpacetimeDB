import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';

import { sha256 } from '../provenance.mjs';
import { RELEASE_MANIFEST_SCHEMA_VERSION, validateReleaseManifest,
  verifyReleaseBundle } from '../release-manifest.mjs';

const roles = ['controller', 'build-sandbox', 'postgres', 'mongodb'];

function fixture(root) {
  const specifications = [
    ['compose.yaml', 'compose'],
    ['deps.tar.zst', 'dependency'],
    ['OPERATOR.md', 'operator-guide'],
    ['secrets.example', 'secrets-template'],
    ['SUPPORT.md', 'support-policy'],
    ...roles.flatMap(role => [[`sbom/${role}.spdx.json`, 'sbom'],
      [`signatures/${role}.sig`, 'signature']]),
  ];
  const files = specifications.map(([path, role]) => {
    const absolute = join(root, ...path.split('/'));
    mkdirSync(dirname(absolute), { recursive: true });
    writeFileSync(absolute, `${path}\n`);
    const content = readFileSync(absolute);
    return { path, role, sha256: sha256(content), bytes: statSync(absolute).size };
  });
  const images = roles.map((role, index) => {
    const digest = String(index + 1).repeat(64);
    return { id: `stack-bench-${role}`, role,
      reference: `registry.example/stack-bench/${role}@sha256:${digest}`, digest,
      platform: 'linux/amd64', sbomPath: `sbom/${role}.spdx.json`,
      signaturePath: `signatures/${role}.sig` };
  });
  return {
    schemaVersion: RELEASE_MANIFEST_SCHEMA_VERSION,
    id: 'stack-bench-v1', version: '1.0.0', state: 'candidate',
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
  };
}

test('release manifest binds every required image, SBOM, signature and operator file', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  try {
    const manifest = fixture(root);
    assert.deepEqual(validateReleaseManifest(manifest), manifest);
    const verified = verifyReleaseBundle(manifest, root);
    assert.equal(verified.ok, true, JSON.stringify(verified.results));
    assert.equal(verified.verificationLevel, 'candidate-file-integrity');
    assert.equal(verified.results.length, manifest.files.length);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('release verification detects changed, missing, and path-escaping files', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  try {
    const changed = fixture(root);
    writeFileSync(join(root, 'compose.yaml'), 'changed');
    assert.equal(verifyReleaseBundle(changed, root).results
      .find(result => result.path === 'compose.yaml').ok, false);

    const missing = fixture(root);
    rmSync(join(root, 'OPERATOR.md'));
    assert.equal(verifyReleaseBundle(missing, root).results
      .find(result => result.path === 'OPERATOR.md').reason, 'missing');

    const escaped = fixture(root);
    escaped.files[0].path = '../compose.yaml';
    assert.throws(() => verifyReleaseBundle(escaped, root), /relative POSIX path/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('release schema refuses mutable images, incomplete roles, insecure destinations and secret paths', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-release-'));
  try {
    const mutable = fixture(root);
    mutable.images[0].reference = 'registry.example/controller:latest';
    assert.throws(() => validateReleaseManifest(mutable), /exact digest/);

    const incomplete = fixture(root);
    incomplete.images.pop();
    assert.throws(() => validateReleaseManifest(incomplete), /missing mongodb/);

    const insecure = fixture(root);
    insecure.outboundDestinations[0].url = 'http://registry.npmjs.org';
    assert.throws(() => validateReleaseManifest(insecure), /must be HTTPS/);

    const secretEscape = fixture(root);
    secretEscape.secrets[0].composeTarget = '/run/secrets/../host';
    assert.throws(() => validateReleaseManifest(secretEscape), /under \/run\/secrets/);

    const prematurelyQualified = fixture(root);
    prematurelyQualified.state = 'qualified';
    assert.throws(() => verifyReleaseBundle(prematurelyQualified, root),
      /cryptographic signature verification/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
