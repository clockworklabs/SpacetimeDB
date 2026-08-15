import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { createRecipeTaskRequest } from '../recipe-selection.mjs';
import { resolveLegacyRecipeRelease } from '../recipe-release.mjs';
import { loadTrack } from '../tracks.mjs';

const ROOT = join(import.meta.dirname, '..');
const AGENT = join(ROOT, 'agent.mjs');
const binding = resolveLegacyRecipeRelease(loadTrack('ecommerce'), 1);

function printPrompt(app, request) {
  return execFileSync(process.execPath, [AGENT, '--mode', 'build', '--backend', 'postgres',
    '--track', 'ecommerce', '--level', '1', '--app', app,
    '--recipe-task-json', JSON.stringify(request), '--print-prompt'], {
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
