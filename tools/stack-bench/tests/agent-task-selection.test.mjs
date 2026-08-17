import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { agentRecipeRequest, agentScenarioPaths } from '../agent.mjs';
import { resolveGuidanceProfile } from '../condition-compiler.mjs';
import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest,
  createRecipeTaskRequest } from '../recipe-selection.mjs';
import { resolveRecipeRelease } from '../recipe-release.mjs';
import { loadTrack } from '../tracks.mjs';

const ROOT = join(import.meta.dirname, '..');
const AGENT = join(ROOT, 'agent.mjs');
const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1);

function printPrompt(app, request, extraArgs = []) {
  return execFileSync(process.execPath, [AGENT, '--mode', 'build', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '1', '--app', app,
    '--recipe-task-json', JSON.stringify(request), ...extraArgs, '--print-prompt'], {
    encoding: 'utf8', stdio: 'pipe',
    env: { ...process.env, STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
}

function printStandalonePrompt(app, extraArgs = []) {
  return execFileSync(process.execPath, [AGENT, '--mode', 'build', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '1', '--app', app, ...extraArgs, '--print-prompt'], {
    encoding: 'utf8', stdio: 'pipe',
    env: { ...process.env, STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
}

test('pack selection changes the real model prompt and exact task identity', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-selected-task-'));
  try {
    const selected = createBoundRecipeTaskRequest(binding,
      { featureIds: ['ecommerce.feature.accounts'] });
    const visible = createAgentVisibleTaskRequest(binding, selected);
    const prompt = printPrompt(app, visible);
    assert.match(prompt, /## Accounts/);
    assert.doesNotMatch(prompt, /## Reviews/);
    assert.doesNotMatch(prompt, /## Cart/);
    assert.deepEqual(selected.selection.promptPacks, ['ecommerce.feature.accounts']);

    const tampered = structuredClone(visible);
    tampered.task.sha256 = '0'.repeat(64);
    assert.throws(() => printPrompt(app, tampered), error =>
      /recipe task changed after request resolution/.test(String(error.stderr)));
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('pack dependencies become requested task scope while checks only narrow measurement', () => {
  const cart = createBoundRecipeTaskRequest(binding,
    { featureIds: ['ecommerce.feature.cart-checkout'] });
  assert.deepEqual(cart.selection.promptPacks, [
    'ecommerce.feature.accounts',
    'ecommerce.feature.cart-checkout',
    'ecommerce.feature.catalog',
  ]);
  assert.match(cart.task.requirementText, /signed-in customer/);
  assert.match(cart.task.requirementText, /## Cart and checkout/);

  const oneKey = binding.release.checkCatalog.find(check =>
    check.packId === 'ecommerce.feature.accounts').stableKey;
  const full = createBoundRecipeTaskRequest(binding);
  const checkOnly = createBoundRecipeTaskRequest(binding, { checkKeys: [oneKey] });
  assert.deepEqual(checkOnly.selection.promptPacks, full.selection.promptPacks);
  assert.equal(checkOnly.selection.checks.length, 1);
  assert.equal(checkOnly.task.requirementText, full.task.requirementText);
});

test('ordinary runs select scored checks while test-development checks require exact selection', () => {
  const ordinary = createRecipeTaskRequest(binding);
  assert.equal(ordinary.selection.checks.length, 46);
  assert.equal(ordinary.selection.checks.every(check => check.points > 0), true);
  assert.equal(ordinary.selection.completeness, 'full');

  const candidate = binding.release.checkCatalog.find(check => check.points === 0);
  assert(candidate);
  const development = createRecipeTaskRequest(binding, { checkKeys: [candidate.stableKey] });
  assert.deepEqual(development.selection.checks.map(check => check.stableKey), [candidate.stableKey]);
  assert.equal(development.selection.scoredPoints, 0);
  assert.equal(development.selection.completeness, 'subset');
});

test('selected pack prompts contain only their own framework-neutral testing calls', () => {
  const candidate = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.3.0');
  const neutral = resolveGuidanceProfile('neutral@1.0.0', ['postgres']);
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-candidate-task-'));
  try {
    const identity = createBoundRecipeTaskRequest(candidate,
      { featureIds: ['ecommerce.feature.accounts'] });
    const prompt = printPrompt(app, createAgentVisibleTaskRequest(candidate, identity), [
      '--guidance', neutral.mode,
      '--guidance-document-json', JSON.stringify(neutral.documents.postgres),
      '--skills-json', JSON.stringify(neutral.skills.postgres.ids),
    ]);
    assert.match(prompt, /POST \/api\/auth\/signup/);
    assert.doesNotMatch(prompt, /\bExpress\b|socket\.io|Drizzle|Prisma/i);
    assert.doesNotMatch(identity.task.requirementText,
      /POST \/api\/checkout|POST \/api\/admin\/restock/);

    const cart = createBoundRecipeTaskRequest(candidate,
      { featureIds: ['ecommerce.feature.cart-checkout'] });
    assert.match(cart.task.requirementText, /POST \/api\/checkout/);
    assert.match(cart.task.requirementText, /POST \/api\/auth\/signin/);
    assert.doesNotMatch(cart.task.requirementText, /POST \/api\/admin\/restock/);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('the real unprescribed prompt withholds every expected quality specification', () => {
  const modular = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.3.0');
  const features = modular.release.components.packs
    .filter(pack => pack.moduleType === 'feature').map(pack => pack.id);
  const expectedSpecifications = modular.release.components.packs
    .filter(pack => pack.moduleType === 'specification')
    .map(pack => `${pack.id}@${pack.version}`);
  const task = createBoundRecipeTaskRequest(modular, {
    featureIds: features,
    expectedSpecifications,
  });
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-modular-task-'));
  try {
    const visible = createAgentVisibleTaskRequest(modular, task);
    const prompt = printPrompt(app, visible);
    assert.match(prompt, /## Accounts/);
    assert.match(prompt, /## Cart and checkout/);
    assert.match(prompt, /## Warehouse administration/);
    assert.doesNotMatch(prompt, /## Access control:|## State durability:|## Live state:|## Concurrency safety:|## Transactional integrity:/);
    assert.doesNotMatch(prompt, /server-enforced authority|survives a page reload|only one customer can receive the last unit|historical order prices do not change/);
    assert.doesNotMatch(JSON.stringify(visible), /ecommerce\.spec/);
    assert.equal(task.selection.scoredPoints, 58);
    assert(task.selection.scoredChecks.some(check => check.treatment === 'expected'));
    assert.deepEqual(visible.selection.requested.checks, []);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('exact modular qualification can include supporting checks without changing the prompt scope', () => {
  const modular = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.3.0');
  const features = modular.release.components.packs
    .filter(pack => pack.moduleType === 'feature').map(pack => pack.id);
  const expectedSpecifications = modular.release.components.packs
    .filter(pack => pack.moduleType === 'specification')
    .map(pack => `${pack.id}@${pack.version}`);
  const ordinary = createBoundRecipeTaskRequest(modular, { featureIds: features,
    expectedSpecifications });
  const exact = createBoundRecipeTaskRequest(modular, { featureIds: features,
    expectedSpecifications, checkKeys: modular.release.checkCatalog.map(check => check.stableKey) });

  assert.equal(ordinary.selection.checks.length, 46);
  assert.equal(ordinary.selection.checks.every(check => check.points > 0), true);
  assert.equal(exact.selection.checks.length, 48);
  assert.equal(exact.selection.scoredPoints, 58);
  assert.equal(exact.selection.checks.filter(check => check.points === 0).length, 2);
  assert.deepEqual(exact.selection.promptPacks, ordinary.selection.promptPacks);
});

test('agent provenance uses the exact recipe execution instead of the legacy level suites', () => {
  const track = loadTrack('ecommerce');
  const modular = resolveRecipeRelease(track, 1, 'ecommerce.l1-modular@2.3.0');
  const paths = agentScenarioPaths(track, 1, modular);

  assert.deepEqual(paths.map(path => path.replaceAll('\\', '/').split('/scenarios/')[1]),
    modular.execution.map(execution => execution.source.split('scenarios/')[1]));
  assert(paths.some(path => path.endsWith('01-last-unit-2.3.0.json')));
  assert.equal(paths.some(path => path.endsWith('01-contention.json')), false);
});

test('a standalone recipe selects its exact prompt and cannot disagree with a bound task', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-standalone-recipe-'));
  try {
    const promoted = printStandalonePrompt(app);
    const candidate = printStandalonePrompt(app,
      ['--recipe', 'ecommerce.l1-modular@2.3.0']);
    assert.equal(candidate, promoted);
    assert.match(candidate, /data-buy-input/);
    assert.match(promoted, /data-buy-input/);

    const request = createRecipeTaskRequest(binding).request;
    assert.throws(() => printPrompt(app, request,
      ['--recipe', 'ecommerce.l1-modular@2.2.0']), error =>
      /does not match bound task/.test(String(error.stderr)));
    assert.equal(agentRecipeRequest('ecommerce.l1-modular@2.3.0'),
      'ecommerce.l1-modular@2.3.0');
  } finally { rmSync(app, { recursive: true, force: true }); }
});
