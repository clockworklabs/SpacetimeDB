import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import test from 'node:test';
import { prepareStateVolume, writeStateSecret } from '../appliance/state-volume.js';
import { DATABASE_IMAGES } from '../src/stacks/database-containers.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compileCampaignFile } from '../src/campaigns/campaign-compiler.js';

test('setup uses the daemon volume path and exact local images without disclosing secrets', () => {
  const calls: readonly string[][] = [];
  const root = '/var/lib/docker/volumes/stack-bench-state/_data';
  const id = `sha256:${'a'.repeat(64)}`;
  const env = prepareStateVolume({}, args => {
    (calls as string[][]).push([...args]);
    if (args[0] === 'version') return 'linux';
    if (args[0] === 'image') return id;
    if (args[0] === 'volume' && args[1] === 'inspect') return root;
    return '';
  });
  assert.match(env, new RegExp(`STACK_BENCH_STATE_ROOT=${root}`));
  assert.match(env, new RegExp(`STACK_BENCH_CONTROLLER_IMAGE=${id}`));
  assert.match(env, /STACK_BENCH_RUNNER_CAPACITY=1/);
  const initialization = calls.find(args => args[0] === 'run');
  assert(initialization?.includes(`type=volume,source=stack-bench-state,target=${root}`));
  assert.doesNotMatch(env, /ANTHROPIC_API_KEY=|CLAUDE_CODE_OAUTH_TOKEN=/);
  assert.equal(calls.filter(args => args[0] === 'pull').length, 0, 'installed exact images need no pull');
  assert.throws(() => prepareStateVolume({}, () => 'windows'), /Linux containers/);
});

test('setup installs an image-bound paid demo and preserves an existing plan', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-setup-'));
  const controller = `sha256:${'a'.repeat(64)}`;
  const build = `sha256:${'b'.repeat(64)}`;
  let initialization: readonly string[] = [];
  try {
    prepareStateVolume({}, args => {
      if (args[0] === 'version') return 'linux';
      if (args[0] === 'image') return args.at(-1) === 'stack-bench-build:local' ? build : controller;
      if (args[0] === 'volume' && args[1] === 'inspect') return '/state';
      if (args[0] === 'run') initialization = args;
      return '';
    });
    const script = initialization[initialization.indexOf('-e') + 1]!;
    const localScript = script.replaceAll('/opt/stack-bench/appliance',
      resolve(STACK_BENCH_ROOT, 'appliance').replaceAll('\\', '/'));
    const initialize = (image: string) => execFileSync(process.execPath,
      ['-e', localScript, root, image, build]);
    initialize(controller);
    assert.equal(readFileSync(join(root, 'results', 'plans', 'demo.json'), 'utf8'),
      readFileSync(resolve(STACK_BENCH_ROOT, 'appliance', 'campaign.demo.json'), 'utf8'));
    const path = join(root, 'results', 'plans', 'paid-l1.json');
    const bytes = readFileSync(path, 'utf8');
    const plan = JSON.parse(bytes);
    assert.equal(plan.state, 'frozen');
    assert.equal(plan.runtime.controllerImage, controller);
    assert.equal(plan.runtime.buildImage, build);
    assert.equal(plan.parallelism, 3);
    assert.equal(plan.budgets.maxCostUsdPerAttempt, 10);
    const compiled = compileCampaignFile(path);
    assert.equal(compiled.state, 'frozen');
    assert.deepEqual(compiled.definition.runtime, plan.runtime);
    initialize(`sha256:${'c'.repeat(64)}`);
    assert.equal(readFileSync(path, 'utf8'), bytes, 'setup must not rebind an existing plan');
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('setup pulls each absent pinned native backend image before state initialization', () => {
  const missing = new Set(Object.values(DATABASE_IMAGES));
  const pulls: string[][] = [];
  prepareStateVolume({}, args => {
    if (args[0] === 'version') return 'linux';
    if (args[0] === 'image') {
      if (missing.has(args.at(-1)!)) throw new Error('No such image');
      return `sha256:${'a'.repeat(64)}`;
    }
    if (args[0] === 'pull') {
      pulls.push([...args]);
      missing.delete(args.at(-1)!);
    }
    if (args[0] === 'volume' && args[1] === 'inspect') return '/var/lib/docker/volumes/stack-bench-state/_data';
    if (args[0] === 'run') assert.equal(missing.size, 0);
    return '';
  });
  assert.deepEqual(pulls, Object.values(DATABASE_IMAGES).map(reference => ['pull', '--platform', 'linux/amd64', reference]));
});

test('a failed native image pull stops setup before it initializes shared state', () => {
  const calls: readonly string[][] = [];
  assert.throws(() => prepareStateVolume({}, args => {
    (calls as string[][]).push([...args]);
    if (args[0] === 'version') return 'linux';
    if (args[0] === 'image' && args.at(-1) === DATABASE_IMAGES.postgres) throw new Error('No such image');
    if (args[0] === 'image') return `sha256:${'a'.repeat(64)}`;
    if (args[0] === 'pull') throw new Error('registry unavailable');
    return '';
  }), /registry unavailable/);
  assert.equal(calls.some(args => ['run', 'volume'].includes(args[0]!)), false);
});

test('secret input is restricted to named private files and never needs a shell argument value', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-secret-'));
  try {
    assert.throws(() => writeStateSecret('../other', 'secret', root), /secret name/);
    assert.throws(() => writeStateSecret('anthropic_api_key', 'a\nb', root), /one non-empty line/);
    assert.throws(() => writeStateSecret('codex_auth', '{invalid', root), /valid JSON/);
    writeStateSecret('codex_auth', '{\n  "auth_mode": "chatgpt"\n}', root);
    assert.equal(readFileSync(join(root, 'secrets', 'codex_auth'), 'utf8'), '{"auth_mode":"chatgpt"}\n');
    writeStateSecret('openrouter_api_key', 'test-openrouter', root);
    assert.equal(readFileSync(join(root, 'secrets', 'openrouter_api_key'), 'utf8'), 'test-openrouter\n');
    writeStateSecret('openai_api_key', 'test-openai', root);
    assert.equal(readFileSync(join(root, 'secrets', 'openai_api_key'), 'utf8'), 'test-openai\n');
    writeStateSecret('anthropic_api_key', '  test-key\n', root);
    const path = join(root, 'secrets', 'anthropic_api_key');
    assert.equal(readFileSync(path, 'utf8'), 'test-key\n');
    if (process.platform !== 'win32') assert.equal(statSync(path).mode & 0o777, 0o600);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
