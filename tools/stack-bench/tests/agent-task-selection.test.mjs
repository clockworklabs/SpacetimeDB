import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

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

test('pack selection changes the real model prompt and exact task identity', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-selected-task-'));
  try {
    const selected = createRecipeTaskRequest(binding,
      { packIds: ['ecommerce.identity-access'] });
    const prompt = printPrompt(app, selected.request);
    assert.match(prompt, /### Accounts/);
    assert.doesNotMatch(prompt, /### Reviews/);
    assert.doesNotMatch(prompt, /### Cart/);
    assert.deepEqual(selected.selection.taskPacks, ['ecommerce.identity-access']);

    const tampered = structuredClone(selected.request);
    tampered.task.sha256 = '0'.repeat(64);
    assert.throws(() => printPrompt(app, tampered), error =>
      /recipe task changed after request resolution/.test(String(error.stderr)));
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('pack dependencies become requested task scope while checks only narrow measurement', () => {
  const durability = createRecipeTaskRequest(binding,
    { packIds: ['ecommerce.session-durability'] });
  assert.deepEqual(durability.selection.taskPacks, [
    'ecommerce.cart-checkout',
    'ecommerce.identity-access',
    'ecommerce.session-durability',
  ]);
  assert.match(durability.task.requirementText, /signed-in session/);
  assert.match(durability.task.requirementText, /### Cart/);

  const oneKey = binding.release.checkCatalog.find(check =>
    check.packId === 'ecommerce.identity-access').stableKey;
  const checkOnly = createRecipeTaskRequest(binding, { checkKeys: [oneKey] });
  assert.equal(checkOnly.selection.taskPacks.length, binding.release.components.packs.length);
  assert.equal(checkOnly.selection.checks.length, 1);
  assert.equal(checkOnly.task.requirementText, binding.plan.recipe.task.requirementText);
});

test('ordinary runs select scored checks while test-development checks require exact selection', () => {
  const ordinary = createRecipeTaskRequest(binding);
  assert.equal(ordinary.selection.checks.length, 39);
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
    'ecommerce.l1-standard@1.1.0');
  const neutral = resolveGuidanceProfile('neutral@1.0.0', ['postgres']);
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-candidate-task-'));
  try {
    const identity = createRecipeTaskRequest(candidate,
      { packIds: ['ecommerce.identity-access'] });
    const prompt = printPrompt(app, identity.request, [
      '--guidance', neutral.mode,
      '--guidance-document-json', JSON.stringify(neutral.documents.postgres),
      '--skills-json', JSON.stringify(neutral.skills.postgres.ids),
    ]);
    assert.match(prompt, /POST \/api\/auth\/signup/);
    assert.doesNotMatch(prompt, /\bExpress\b|socket\.io|Drizzle|Prisma/i);
    assert.doesNotMatch(identity.task.requirementText,
      /POST \/api\/checkout|POST \/api\/admin\/restock/);

    const cart = createRecipeTaskRequest(candidate,
      { packIds: ['ecommerce.cart-checkout'] });
    assert.match(cart.task.requirementText, /POST \/api\/checkout/);
    assert.doesNotMatch(cart.task.requirementText, /POST \/api\/auth\/signin|POST \/api\/admin\/restock/);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('the real unprescribed prompt withholds every expected quality specification', () => {
  const modular = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.l1-modular@2.1.0');
  const features = modular.release.components.packs
    .filter(pack => pack.moduleType === 'feature').map(pack => pack.id);
  const expectedSpecifications = modular.release.components.packs
    .filter(pack => pack.moduleType === 'specification'
      && pack.id !== 'ecommerce.spec.external-data-sync')
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
    assert.equal(task.selection.scoredPoints, 44);
    assert(task.selection.scoredChecks.some(check => check.treatment === 'expected'));
    assert.deepEqual(visible.selection.requested.checks, []);
  } finally { rmSync(app, { recursive: true, force: true }); }
});
