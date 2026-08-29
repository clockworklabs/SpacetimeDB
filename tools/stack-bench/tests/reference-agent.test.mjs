import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { deployReferenceAndRestoreSource, parseReferenceAgentArgs, prepareReferenceSource, referenceDevCommand,
  restoreReferenceSourceIdentity } from '../dist/src/references/reference-agent.mjs';

function argv({ mode = 'build', level = '2', runIndex = '0', recipe } = {}) {
  return ['node', 'reference-agent.mjs', '--mode', mode, '--backend', 'mongodb',
    '--app', '/work/reference', '--track', 'ecommerce', '--level', level,
    '--run-index', runIndex, ...(recipe ? ['--recipe', recipe] : [])];
}

test('the model-free reference builder accepts any explicit positive level', () => {
  assert.equal(parseReferenceAgentArgs(argv({ level: '1' })).level, 1);
  assert.equal(parseReferenceAgentArgs(argv({ level: '2' })).level, 2);
  assert.equal(parseReferenceAgentArgs(argv({ level: '3' })).level, 3);
  assert.equal(parseReferenceAgentArgs(argv({ mode: 'upgrade', level: '4' })).mode, 'upgrade');
  assert.equal(parseReferenceAgentArgs(argv({ mode: 'fix', level: '4' })).mode, 'fix');
});

test('the shared adapter request forwards the exact recipe into reference selection', async () => {
  const { agentRequestArgv } = await import('../dist/src/agents/agent-adapter-contract.js');
  const { AGENT_ADAPTER_REGISTRY } = await import('../src/agents/agent-adapters.mjs');
  const command = agentRequestArgv(AGENT_ADAPTER_REGISTRY.get('reference-fixture'), {
    mode: 'build', backend: 'mongodb', level: 1, app: '/work/reference',
    track: 'ecommerce', runIndex: 0, model: 'reference-fixture',
    guidance: 'prescribed', recipe: 'ecommerce.l1-modular@2.5.0',
  });
  const parsed = parseReferenceAgentArgs(['node', ...command]);
  assert.equal(parsed.recipe, 'ecommerce.l1-modular@2.5.0');
});

test('the model-free reference builder rejects unsupported modes and malformed scope', () => {
  assert.throws(() => parseReferenceAgentArgs(argv({ mode: 'resume' })),
    /only build, upgrade and fix modes/);
  assert.throws(() => parseReferenceAgentArgs(argv({ level: '0' })), /positive integer level/);
  assert.throws(() => parseReferenceAgentArgs(argv({ level: '1.5' })), /positive integer level/);
  assert.throws(() => parseReferenceAgentArgs(argv({ runIndex: '-1' })), /non-negative integer run-index/);
});

test('dependency progression seeds once and verifies the same full fixture on later levels', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-progression-'));
  try {
    const app = join(root, 'app');
    const common = { mode: 'build', backend: 'mongodb', track: 'ecommerce', level: 1,
      recipe: 'ecommerce.progression-catalog@1.0.0', app };
    const fresh = prepareReferenceSource(common);
    assert.equal(fresh.fixture.id, 'ecommerce-progression-mongodb');
    assert.equal(fresh.seeded, true);

    const upgraded = prepareReferenceSource({ ...common, mode: 'upgrade', level: 2 });
    assert.equal(upgraded.fixture.id, fresh.fixture.id);
    assert.equal(upgraded.sourceSha256, fresh.sourceSha256);
    assert.equal(upgraded.seeded, false);

    const repaired = prepareReferenceSource({ ...common, mode: 'fix', level: 2 });
    assert.equal(repaired.fixture.id, fresh.fixture.id);
    assert.equal(repaired.sourceSha256, fresh.sourceSha256);
    assert.equal(repaired.seeded, false);

    const dependencyDirectory = join(app, 'client', 'node_modules', '.bin');
    mkdirSync(dependencyDirectory, { recursive: true });
    try {
      symlinkSync(join(app, 'client', 'package.json'), join(dependencyDirectory, 'package-link'), 'file');
      const afterInstall = prepareReferenceSource({ ...common, mode: 'upgrade', level: 2 });
      assert.equal(afterInstall.sourceSha256, fresh.sourceSha256);
    } catch (error) {
      if (!['EPERM', 'EACCES'].includes(error.code)) throw error;
    }

    writeFileSync(join(app, 'unexpected.txt'), 'not part of the fixture');
    assert.throws(() => prepareReferenceSource({ ...common, mode: 'upgrade', level: 3 }),
      /contains source other than/);

    const empty = join(root, 'empty');
    assert.throws(() => prepareReferenceSource({ ...common, mode: 'upgrade', level: 2, app: empty }),
      /upgrade requires the existing ecommerce-progression-mongodb source/);
    assert.throws(() => prepareReferenceSource({ ...common, mode: 'fix', level: 2,
      app: join(root, 'empty-fix') }),
    /fix requires the existing ecommerce-progression-mongodb source/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference deployment restores canonical source while retaining generated bindings', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-identity-'));
  try {
    const app = join(root, 'app');
    const prepared = prepareReferenceSource({
      mode: 'build', backend: 'spacetime', track: 'ecommerce', level: 1,
      recipe: 'ecommerce.progression-catalog@1.0.0', app,
    });
    const lock = join(app, 'client', 'package-lock.json');
    const canonicalLock = readFileSync(lock, 'utf8');
    writeFileSync(lock, 'runtime-generated lockfile\n');
    const bindings = join(app, 'client', 'src', 'module_bindings', 'index.ts');
    mkdirSync(join(app, 'client', 'src', 'module_bindings'), { recursive: true });
    writeFileSync(bindings, 'export const generated = true;\n');
    const viteCache = join(app, 'client', 'node_modules', '.vite', 'deps_temp', 'package.json');
    mkdirSync(join(app, 'client', 'node_modules', '.vite', 'deps_temp'), { recursive: true });
    writeFileSync(viteCache, '{"type":"module"}\n');

    const restored = restoreReferenceSourceIdentity(prepared.fixture, app);
    assert.equal(restored.sha256, prepared.sourceSha256);
    assert.equal(readFileSync(lock, 'utf8'), canonicalLock);
    assert.equal(readFileSync(bindings, 'utf8'), 'export const generated = true;\n');
    assert.equal(readFileSync(viteCache, 'utf8'), '{"type":"module"}\n');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('reference deployment restores source after a failed deploy', async () => {
  const events = [];
  await assert.rejects(() => deployReferenceAndRestoreSource(async () => {
    events.push('deploy');
    throw new Error('deploy failed');
  }, () => events.push('restore')), /deploy failed/);
  assert.deepEqual(events, ['deploy', 'restore']);
});

test('reference deployment reports both deploy and restoration failures', async () => {
  await assert.rejects(() => deployReferenceAndRestoreSource(async () => {
    throw new Error('deploy failed');
  }, () => { throw new Error('restore failed'); }), error => {
    assert.equal(error instanceof AggregateError, true);
    assert.deepEqual(error.errors.map(item => item.message), ['deploy failed', 'restore failed']);
    return true;
  });
});

test('reference clients are explicitly reachable outside their build container', () => {
  const prefix = 'umask 000; exec /usr/bin/setpriv --reuid=10001 --regid=10001 --init-groups '
    + '/usr/local/bin/npm run ';
  assert.equal(referenceDevCommand('reference-server'),
    `${prefix}dev > /run/application/reference-server.log 2>&1`);
  assert.equal(referenceDevCommand('reference-server', { script: 'start' }),
    `${prefix}start > /run/application/reference-server.log 2>&1`);
  assert.equal(referenceDevCommand('reference-client', { networkVisible: true }),
    `${prefix}dev -- --host 0.0.0.0 > /run/application/reference-client.log 2>&1`);
  assert.equal(referenceDevCommand('reference-client', { networkVisible: true, port: 6475 }),
    `${prefix}dev -- --host 0.0.0.0 --port 6475 --strictPort `
      + '> /run/application/reference-client.log 2>&1');
  assert.throws(() => referenceDevCommand('reference-client', { port: 0 }),
    /invalid reference port 0/);
  assert.throws(() => referenceDevCommand('../unsafe'), /unsafe reference log name/);
  assert.throws(() => referenceDevCommand('reference-server', { script: 'start; unsafe' }),
    /unsafe reference script/);
});

test('reference adapter seeds an empty campaign app from the exact registered fixture', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-'));
  try {
    const args = { backend: 'mongodb', track: 'ecommerce', level: 1,
      app: join(root, 'app') };
    const seeded = prepareReferenceSource(args);
    assert.equal(seeded.fixture.id, 'ecommerce-l1-action-inputs-2.4-mongodb');
    assert.equal(seeded.seeded, true);
    assert.equal(prepareReferenceSource(args).seeded, false);
    writeFileSync(join(args.app, 'unexpected.txt'), 'different source');
    assert.throws(() => prepareReferenceSource(args), /contains source other than/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('a retired recipe-specific reference cannot be launched', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-derived-'));
  try {
    const args = { backend: 'mongodb', track: 'ecommerce', level: 1,
      recipe: 'ecommerce.l1-modular@2.3.0', app: join(root, 'app') };
    assert.throws(() => prepareReferenceSource(args),
      /no recipe release|retired|requires exactly one catalogued/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('the promoted L1 2.5 release uses its separately bound action-input fixture', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-l1-2.4-'));
  try {
    const args = { backend: 'mongodb', track: 'ecommerce', level: 1,
      recipe: 'ecommerce.l1-modular@2.5.0', app: join(root, 'app') };
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

test('the promoted L2 release prepares the exact seven action inputs for every backend', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-reference-agent-l2-derived-'));
  const expected = {
    mongodb: '7f2fb4f996d4a9a448d203cd48f4deedeaea7af9fbbf3bb56122b3d4cc546634',
    postgres: '7b3d6c936623ed8ff8b4ff3e61204c291b784c2d36c2bc1edbd5bcca47f9c952',
    spacetime: 'b0bfc4405e684511874f5a867a5dc84e28b258e46729b7199a1e8aa5e27b61ce',
  };
  try {
    for (const [backend, sourceSha256] of Object.entries(expected)) {
      const args = { backend, track: 'ecommerce', level: 2,
        recipe: 'ecommerce.l2-standard@1.6.0', app: join(root, backend) };
      const seeded = prepareReferenceSource(args);
      assert.equal(seeded.fixture.id, `ecommerce-l2-cumulative-1.5-${backend}`);
      assert.equal(seeded.sourceSha256, sourceSha256);
      const files = backend === 'spacetime'
        ? ['client/src/components/ItemCard.tsx', 'client/src/components/OrdersPanel.tsx',
          'client/src/components/AdminPanel.tsx', 'client/src/components/CartPanel.tsx']
        : ['client/src/App.tsx'];
      const client = files.map(path => readFileSync(join(args.app, ...path.split('/')), 'utf8')).join('\n');
      for (const attribute of [
        'data-buy-input=', 'data-ship-input=', 'data-cancel-input=', 'data-transfer-input=',
        'data-restock-input=', 'data-price-input=', 'data-cart-input=',
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
