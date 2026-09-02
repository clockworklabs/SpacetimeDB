import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { campaignExecutionEnvironment, campaignSlotEnvironment }
  from '../src/campaigns/campaign-runtime.js';
import { currentEngineIdentity } from '../src/evidence/artifacts.js';
import { sha256 } from '../src/evidence/provenance.js';

function frozenRuntime(root: string) {
  const digests = {
    controller: 'b'.repeat(64),
    'build-sandbox': 'c'.repeat(64),
    postgres: 'd'.repeat(64),
    mongodb: 'e'.repeat(64),
  };
  const images = Object.entries(digests).map(([role, digest]) => ({
    id: `stack-bench-${role}`,
    role,
    reference: `registry.example/stack-bench/${role}@sha256:${digest}`,
    digest,
    platform: 'linux/amd64',
    sbomPath: `sbom/${role}.spdx.json`,
  }));
  const file = (path: string, role: string) => ({ path, role, sha256: 'f'.repeat(64), bytes: 1 });
  const manifest = {
    schemaVersion: 2,
    id: 'stack-bench-v1',
    version: '1.0.0',
    state: 'candidate',
    sourceRevision: 'a'.repeat(40),
    sourceSha256: 'a'.repeat(64),
    supportedRunner: { os: 'linux', architecture: 'amd64', stateRoot: '/var/lib/stack-bench',
      networkMode: 'host', dockerSocket: true },
    images,
    files: [file('compose.yaml', 'compose'), file('deps.tar.zst', 'dependency'),
      file('OPERATOR.md', 'operator-guide'), file('secrets.example', 'secrets-template'),
      file('SUPPORT.md', 'support-policy'),
      ...images.map(image => file(image.sbomPath, 'sbom'))],
    outboundDestinations: [],
    secrets: [],
    signing: null,
  };
  const content = `${JSON.stringify(manifest, null, 2)}\n`;
  const path = join(root, 'release.json');
  writeFileSync(path, content);
  return { path, manifest, runtime: {
    releaseManifestSha256: sha256(content),
    controllerImage: images.find(image => image.role === 'controller')!.reference,
    buildImage: images.find(image => image.role === 'build-sandbox')!.reference,
    platform: 'linux/amd64',
  } };
}

test('parallel SpacetimeDB slots receive distinct dedicated host ports', () => {
  assert.equal(campaignSlotEnvironment({}, 'spacetime', 0).STACK_BENCH_STDB_URI,
    'http://127.0.0.1:3210');
  assert.equal(campaignSlotEnvironment({}, 'spacetime', 7).STACK_BENCH_STDB_URI,
    'http://127.0.0.1:3217');
  assert.equal(campaignSlotEnvironment({ STACK_BENCH_STDB_URI: 'http://localhost:4100' },
    'spacetime', 2).STACK_BENCH_STDB_URI, 'http://localhost:4102');
  assert.equal(campaignSlotEnvironment({ KEEP: 'yes' }, 'postgres', 2).KEEP, 'yes');
  assert.throws(() => campaignSlotEnvironment({ STACK_BENCH_STDB_URI: 'https://example.com' },
    'spacetime', 1), /serverUri must use http|loopback/);
});

test('a frozen campaign proves its release and both runtime images before admission', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-campaign-runtime-'));
  try {
    const { path, manifest, runtime } = frozenRuntime(root);
    const plan = { state: 'frozen', identities: { engine: currentEngineIdentity() },
      definition: { runtime } };
    const env = { STACK_BENCH_CONTROLLER_IMAGE: runtime.controllerImage,
      STACK_BENCH_RELEASE_MANIFEST: path };
    assert.equal(campaignExecutionEnvironment(plan, env).STACK_BENCH_IMAGE, runtime.buildImage);
    const internalPlan = { state: 'frozen', identities: plan.identities, definition: { runtime: {
      ...runtime, releaseManifestSha256: null,
    } } };
    assert.equal(campaignExecutionEnvironment(internalPlan, {
      STACK_BENCH_CONTROLLER_IMAGE: runtime.controllerImage,
    }).STACK_BENCH_IMAGE, runtime.buildImage);
    assert.throws(() => campaignExecutionEnvironment(plan, { ...env,
      STACK_BENCH_CONTROLLER_IMAGE: `registry.example/controller@sha256:${'1'.repeat(64)}` }),
    /controller image does not match/);
    assert.throws(() => campaignExecutionEnvironment({ ...plan,
      identities: { engine: { ...plan.identities.engine, sha256: '0'.repeat(64) } } }, env),
    /engine does not match/);
    assert.throws(() => campaignExecutionEnvironment(plan, { ...env,
      STACK_BENCH_IMAGE: `registry.example/build@sha256:${'2'.repeat(64)}` }), /conflicts/);
    const wrongImages = structuredClone(manifest);
    const wrongController = wrongImages.images.find(image => image.role === 'controller')!;
    wrongController.digest = '1'.repeat(64);
    wrongController.reference = `registry.example/stack-bench/controller@sha256:${wrongController.digest}`;
    const wrongContent = `${JSON.stringify(wrongImages, null, 2)}\n`;
    writeFileSync(path, wrongContent);
    assert.throws(() => campaignExecutionEnvironment({ state: 'frozen', identities: plan.identities,
      definition: {
      runtime: { ...runtime, releaseManifestSha256: sha256(wrongContent) },
    } }, env), /release manifest images do not match/);
    writeFileSync(path, '{}\n');
    assert.throws(() => campaignExecutionEnvironment(plan, env), /release manifest does not match/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
