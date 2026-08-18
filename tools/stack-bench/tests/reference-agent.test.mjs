import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { parseReferenceAgentArgs, prepareReferenceSource,
  referenceDevCommand } from '../src/references/reference-agent.mjs';

function argv({ mode = 'build', level = '2', runIndex = '0', recipe } = {}) {
  return ['node', 'reference-agent.mjs', '--mode', mode, '--backend', 'mongodb',
    '--app', '/work/reference', '--track', 'ecommerce', '--level', level,
    '--run-index', runIndex, ...(recipe ? ['--recipe', recipe] : [])];
}

test('the model-free reference builder accepts any explicit positive level', () => {
  assert.equal(parseReferenceAgentArgs(argv({ level: '1' })).level, 1);
  assert.equal(parseReferenceAgentArgs(argv({ level: '2' })).level, 2);
  assert.equal(parseReferenceAgentArgs(argv({ level: '3' })).level, 3);
});

test('the shared adapter request forwards the exact recipe into reference selection', async () => {
  const { agentRequestArgv } = await import('../src/agents/agent-adapter-contract.mjs');
  const { AGENT_ADAPTER_REGISTRY } = await import('../src/agents/agent-adapters.mjs');
  const command = agentRequestArgv(AGENT_ADAPTER_REGISTRY.get('reference-fixture'), {
    mode: 'build', backend: 'mongodb', level: 1, app: '/work/reference',
    track: 'ecommerce', runIndex: 0, model: 'reference-fixture',
    guidance: 'prescribed', recipe: 'ecommerce.l1-modular@2.3.0',
  });
  const parsed = parseReferenceAgentArgs(['node', ...command]);
  assert.equal(parsed.recipe, 'ecommerce.l1-modular@2.3.0');
});

test('the model-free reference builder rejects unsupported modes and malformed scope', () => {
  assert.throws(() => parseReferenceAgentArgs(argv({ mode: 'upgrade' })), /only build mode/);
  assert.throws(() => parseReferenceAgentArgs(argv({ level: '0' })), /positive integer level/);
  assert.throws(() => parseReferenceAgentArgs(argv({ level: '1.5' })), /positive integer level/);
  assert.throws(() => parseReferenceAgentArgs(argv({ runIndex: '-1' })), /non-negative integer run-index/);
});

test('reference clients are explicitly reachable outside their build container', () => {
  assert.equal(referenceDevCommand('reference-server'),
    'exec npm run dev > /tmp/reference-server.log 2>&1');
  assert.equal(referenceDevCommand('reference-client', { networkVisible: true }),
    'exec npm run dev -- --host 0.0.0.0 > /tmp/reference-client.log 2>&1');
  assert.throws(() => referenceDevCommand('../unsafe'), /unsafe reference log name/);
});

test('reference adapter seeds an empty campaign app from the exact registered fixture', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-'));
  try {
    const args = { backend: 'mongodb', track: 'ecommerce', level: 1,
      app: join(root, 'app') };
    const seeded = prepareReferenceSource(args);
    assert.equal(seeded.fixture.id, 'ecommerce-l1-direct-actions-mongodb');
    assert.equal(seeded.seeded, true);
    assert.equal(prepareReferenceSource(args).seeded, false);
    writeFileSync(join(args.app, 'unexpected.txt'), 'different source');
    assert.throws(() => prepareReferenceSource(args), /contains source other than/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a recipe-specific reference applies its exact patch without changing the qualified base', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-derived-'));
  try {
    const args = { backend: 'mongodb', track: 'ecommerce', level: 1,
      recipe: 'ecommerce.l1-modular@2.3.0', app: join(root, 'app') };
    const seeded = prepareReferenceSource(args);
    assert.equal(seeded.fixture.id, 'ecommerce-l1-direct-actions-mongodb');
    assert.equal(seeded.sourceSha256,
      'd90ea9c8326202a76bf570d0eb7c716531e3e6e3eb4a4678c677783e9d5dbb40');
    const client = readFileSync(join(args.app, 'client', 'src', 'App.tsx'), 'utf8');
    assert.match(client, /data-buy-input=/);
    assert.match(client, /data-restock-input=/);
    assert.equal(prepareReferenceSource(args).seeded, false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the L1 2.4 candidate uses its separately bound action-input fixture', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-l1-2.4-'));
  try {
    const args = { backend: 'mongodb', track: 'ecommerce', level: 1,
      recipe: 'ecommerce.l1-modular@2.4.0', app: join(root, 'app') };
    const seeded = prepareReferenceSource(args);
    assert.equal(seeded.fixture.id, 'ecommerce-l1-action-inputs-2.4-mongodb');
    assert.equal(seeded.sourceSha256,
      '76810d72211fc0182aa31b663ffc153a82ff1918cd34902187873a4b53a4ebf2');
    const client = readFileSync(join(args.app, 'client', 'src', 'App.tsx'), 'utf8');
    for (const attribute of ['data-buy-input=', 'data-cart-input=', 'data-restock-input=']) {
      assert.match(client, new RegExp(attribute));
    }
    assert.equal(prepareReferenceSource(args).seeded, false);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the L2 candidate prepares the exact six action inputs for every backend', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-l2-derived-'));
  const expected = {
    mongodb: '3c5dd399671c89d59c05f0834d3e24fe9d091fe406a1f4b23b9495411d1010cb',
    postgres: '99c8af18eb709aebfe8bc8d22b55aa13219c0c2dcc7fef07e384ebe5aaa8677a',
    spacetime: '359ed185c5d9dff2754f806c1169314985baeb2bca90a09f2ab52a5ee056b8fc',
  };
  try {
    for (const [backend, sourceSha256] of Object.entries(expected)) {
      const args = { backend, track: 'ecommerce', level: 2,
        recipe: 'ecommerce.l2-standard@1.4.0', app: join(root, backend) };
      const seeded = prepareReferenceSource(args);
      assert.equal(seeded.fixture.id, `ecommerce-l2-server-actions-${backend}`);
      assert.equal(seeded.sourceSha256, sourceSha256);
      const files = backend === 'spacetime'
        ? ['client/src/components/ItemCard.tsx', 'client/src/components/OrdersPanel.tsx',
          'client/src/components/AdminPanel.tsx']
        : ['client/src/App.tsx'];
      const client = files.map(path => readFileSync(join(args.app, ...path.split('/')), 'utf8')).join('\n');
      for (const attribute of [
        'data-buy-input=', 'data-ship-input=', 'data-cancel-input=', 'data-transfer-input=',
        'data-restock-input=', 'data-price-input=',
      ]) assert.match(client, new RegExp(attribute));
      const verified = prepareReferenceSource(args);
      assert.equal(verified.seeded, false);
      assert.equal(verified.sourceSha256, sourceSha256);
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference seeding rejects an unbound link in an otherwise empty destination', t => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-link-'));
  try {
    const app = join(root, 'app');
    mkdirSync(app);
    try { symlinkSync(join(root, 'missing-target'), join(app, 'unchecked-link'), 'file'); }
    catch (error) {
      if (['EPERM', 'EACCES'].includes(error.code)) { t.skip('filesystem cannot create test symlinks'); return; }
      throw error;
    }
    assert.throws(() => prepareReferenceSource({
      backend: 'mongodb', track: 'ecommerce', level: 1, app,
    }), /unsupported filesystem entry/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
