import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { agentRecipeRequest, agentScenarioPaths } from '../commands/agent.js';
import { agentVisibleContractText, contractControlIds }
  from '../src/composition/agent-visible-contract.js';
import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';
import { createAgentVisibleTaskRequest, createBoundRecipeTaskRequest,
  createRecipeTaskRequest, isModularRecipeTaskRequest } from '../src/composition/recipe-selection.js';
import { requireRecipeRelease as resolveRecipeRelease } from '../src/composition/recipe-release.js';
import { loadTrack } from '../src/composition/tracks.js';

const ROOT = STACK_BENCH_ROOT;
const AGENT = join(ROOT, 'dist', 'commands', 'agent.js');
const binding = resolveRecipeRelease(loadTrack('ecommerce'), 1);

type UnknownRecord = Record<string, unknown>;
type VisibleTaskRequest = { task: { sha256: string }; selection: { requested: { checks: string[] } } };

const record = (value: unknown): value is UnknownRecord =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

function assertVisibleTaskRequest(value: unknown): asserts value is VisibleTaskRequest {
  if (!record(value) || !record(value.task) || typeof value.task.sha256 !== 'string'
    || !record(value.selection) || !record(value.selection.requested)
    || !Array.isArray(value.selection.requested.checks)
    || value.selection.requested.checks.some(check => typeof check !== 'string')) {
    throw new Error('agent-visible task request is invalid');
  }
}

function commandStderr(error: unknown): string {
  if (!record(error) || !('stderr' in error)) return '';
  return String(error.stderr);
}

test('selected contract linting uses only declared control ids', () => {
  assert.deepEqual(contractControlIds([
    '| Element ID | Element |',
    '|---|---|',
    '| `staff-link` | staff link |',
    '| `staff-signin-submit` | sign-in button |',
    'The password is `stackbench-staff-2026`.',
    'The attribute is `data-action-input`.',
  ].join('\n')), ['staff-link', 'staff-signin-submit']);
});

test('agent-facing documents reject internal evaluation language', () => {
  assert.equal(agentVisibleContractText('Expose the account controls.'),
    'Expose the account controls.');
  for (const source of [
    'Stack Bench checks this.',
    'The grader checks this.',
    'The automated harness checks this.',
    'Expose this test action.',
    'Use data-testid="account-name".',
  ]) {
    assert.throws(() => agentVisibleContractText(source), /contains internal language/);
  }
});

test('agent-visible contracts include only the selected stack section', () => {
  const source = [
    '<!-- interface:http -->HTTP contract<!-- /interface -->',
    '<!-- interface:reducer -->reducer contract<!-- /interface -->',
  ].join('\n');
  assert.equal(agentVisibleContractText(source, {}, 'http').trim(), 'HTTP contract');
  assert.equal(agentVisibleContractText(source, {}, 'reducer').trim(), 'reducer contract');
  assert.throws(() => agentVisibleContractText(source), /requires a selected application interface/);
  assert.throws(() => agentVisibleContractText('<!-- interface:other -->wrong<!-- /interface -->', {}, 'http'),
    /invalid markers/);
  assert.throws(() => agentVisibleContractText('<!-- interface:http -->broken', {}, 'http'),
    /invalid markers/);
  assert.throws(() => agentVisibleContractText([
    '<!-- interface:http -->outer',
    '<!-- interface:reducer -->nested<!-- /interface -->',
    '<!-- /interface -->',
  ].join('\n'), {}, 'http'), /invalid markers/);
});

function printPrompt(app: string, request: unknown, extraArgs: readonly string[] = []): string {
  return execFileSync(process.execPath, [AGENT, '--mode', 'build', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '1', '--app', app,
    '--recipe-task-json', JSON.stringify(request), ...extraArgs, '--print-prompt'], {
    encoding: 'utf8', stdio: 'pipe',
    env: { ...process.env, STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
}

function printFixPrompt(app: string, request: unknown): string {
  return execFileSync(process.execPath, [AGENT, '--mode', 'fix', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '1', '--app', app,
    '--recipe-task-json', JSON.stringify(request), '--print-prompt'], {
    encoding: 'utf8', stdio: 'pipe',
    env: { ...process.env, STACK_BENCH_IMAGE: 'prompt-review-does-not-use-docker' },
  });
}

function printStandalonePrompt(app: string, extraArgs: readonly string[] = []): string {
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
    assert.doesNotMatch(prompt,
      /harness|runner|testing hooks|automated verification|test interface contract|\bhooks\b/i);
    assert.doesNotMatch(prompt, /check-app|contract check|automated verification|data-testid/i);
    assert.deepEqual(selected.selection.promptPacks, ['ecommerce.feature.accounts']);

    const tampered = structuredClone(visible);
    assertVisibleTaskRequest(tampered);
    tampered.task.sha256 = '0'.repeat(64);
    assert.throws(() => printPrompt(app, tampered), error =>
      /recipe task changed after request resolution/.test(commandStderr(error)));
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('repair prompts retain the selected feature request and public interface', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-repair-task-'));
  try {
    const selected = createBoundRecipeTaskRequest(binding,
      { featureIds: ['ecommerce.feature.accounts'] });
    const prompt = printFixPrompt(app, createAgentVisibleTaskRequest(binding, selected));
    assert.match(prompt, /## Accounts/);
    assert.match(prompt, /## Application interface/);
    assert.doesNotMatch(prompt, /## Reviews|## Cart/);
    assert.doesNotMatch(prompt, /grader|harness|automated verification|score/i);
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
    check.packId === 'ecommerce.feature.accounts');
  assert(oneKey);
  const full = createBoundRecipeTaskRequest(binding);
  const checkOnly = createBoundRecipeTaskRequest(binding, { checkKeys: [oneKey.stableKey] });
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

test('selected pack prompts contain only their own framework-neutral named actions', () => {
  const candidate = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.sequential-l1@2.5.0');
  const neutral = resolveGuidanceProfile('neutral@1.7.0', ['postgres']);
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-candidate-task-'));
  try {
    const identity = createBoundRecipeTaskRequest(candidate,
      { featureIds: ['ecommerce.feature.accounts'] });
    const document = neutral.documents.postgres;
    const skills = neutral.skills.postgres;
    assert(document);
    assert(skills);
    const prompt = printPrompt(app, createAgentVisibleTaskRequest(candidate, identity), [
      '--guidance', neutral.mode,
      '--guidance-document-json', JSON.stringify(document),
      '--skills-json', JSON.stringify(skills.ids),
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
    'ecommerce.sequential-l1@2.5.0');
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
    assert(isModularRecipeTaskRequest(task));
    const prompt = printPrompt(app, visible);
    assert.match(prompt, /## Accounts/);
    assert.match(prompt, /## Cart and checkout/);
    assert.match(prompt, /## Warehouse administration/);
    assert.doesNotMatch(prompt, /## Access control:|## State durability:|## Live state:|## Concurrency safety:|## Transactional integrity:/);
    assert.doesNotMatch(prompt, /server-enforced authority|survives a page reload|only one customer can receive the last unit|historical order prices do not change/);
    assert.doesNotMatch(JSON.stringify(visible), /ecommerce\.spec/);
    assert.equal(task.selection.scoredPoints, 58);
    assert(task.selection.scoredChecks.some(check => check.treatment === 'expected'));
    assertVisibleTaskRequest(visible);
    assert.deepEqual(visible.selection.requested.checks, []);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('exact modular qualification can include supporting checks without changing the prompt scope', () => {
  const modular = resolveRecipeRelease(loadTrack('ecommerce'), 1,
    'ecommerce.sequential-l1@2.5.0');
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

test('agent provenance uses the exact recipe execution instead of level suites', () => {
  const track = loadTrack('ecommerce');
  const modular = resolveRecipeRelease(track, 1, 'ecommerce.sequential-l1@2.5.0');
  const paths = agentScenarioPaths(track, 1, modular);

  assert.deepEqual(paths.map(path => path.replaceAll('\\', '/').split('/scenarios/')[1]),
    modular.execution.map(execution => execution.source?.split('scenarios/')[1]));
  assert(paths.some(path => path.endsWith('01-last-unit-2.3.0.json')));
  assert.equal(paths.some(path => path.endsWith('01-contention.json')), false);
});

test('a standalone recipe selects its exact prompt and cannot disagree with a bound task', () => {
  const app = mkdtempSync(join(tmpdir(), 'stack-bench-standalone-recipe-'));
  try {
    const promoted = printStandalonePrompt(app);
    const candidate = printStandalonePrompt(app,
      ['--recipe', 'ecommerce.sequential-l1@2.5.0']);
    assert.equal(candidate, promoted);
    assert.match(candidate, /data-buy-input/);
    assert.match(promoted, /data-buy-input/);

    const request = createRecipeTaskRequest(binding).request;
    assert.throws(() => printPrompt(app, request,
      ['--recipe', 'ecommerce.sequential-l2@1.6.0']), error =>
      /does not match bound task/.test(commandStderr(error)));
    assert.equal(agentRecipeRequest('ecommerce.sequential-l1@2.5.0'),
      'ecommerce.sequential-l1@2.5.0');
  } finally { rmSync(app, { recursive: true, force: true }); }
});
