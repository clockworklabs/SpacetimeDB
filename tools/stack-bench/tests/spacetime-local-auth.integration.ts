import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { loadTrack } from '../src/composition/tracks.js';
import { resolveGradeRecipeArtifactBinding } from '../src/composition/recipe-release.js';
import { selectScenarioChecks } from '../src/composition/recipe-selection.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// Opt-in integration check against an owned, already running reference. This
// exercises the normal scenarios; it does not create a qualification receipt.
const url = process.env.STACK_BENCH_LOCAL_AUTH_URL;
test('local SpacetimeDB accounts pass the normal account and purchase-session scenarios', { skip: !url }, async t => {
  const spacetime = JSON.parse(process.env.STACK_BENCH_LOCAL_AUTH_TARGET!);
  const variant = process.env.STACK_BENCH_LOCAL_AUTH_CONTROL ?? 'correct';
  assert(['correct', 'ignore-password', 'reject-login'].includes(variant));
  const browser = await chromium.launch({ headless: true });
  const track = loadTrack('ecommerce');
  const results: Array<{ name: string; variant: string; result: Awaited<ReturnType<typeof gradeFeature>> }> = [];
  const scenarios = variant === 'correct'
    ? ['01-account-create', '01-account-duplicate', '01-account-password', '01-account-reload', '01-account-signout', '01-purchase-session']
    : [variant === 'ignore-password' ? '01-purchase-session' : '01-account-signout'];
  try {
    for (const [index, name] of scenarios.entries()) {
      await t.test(name, async () => {
        const path = join(STACK_BENCH_ROOT, `tracks/ecommerce/scenarios/${name}.json`);
        const compiled = compileScenarioDefinition(JSON.parse(readFileSync(path, 'utf8')), { source: path });
        const binding = resolveGradeRecipeArtifactBinding(track, 1, path, null);
        const scenario = selectScenarioChecks(compiled, binding?.release ?? null, []);
        const result = await gradeFeature(browser, scenario.features[0]!,
          { url: url!, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
          { runId: `auth${Date.now().toString(36)}${index}`, roomName: name => name, url: url!,
            actions: track.actions, spacetime, backend: 'spacetime', nullControl: false });
        results.push({ name, variant, result });
        assert.equal(result.setupEvidence.status, 'passed', JSON.stringify(result));
        assert(result.criteria.length > 0);
        for (const criterion of result.criteria) {
          assert.equal(criterion.evidence.status, variant === 'correct' ? 'passed' : 'failed', JSON.stringify(result));
        }
      });
    }
  } finally {
    await browser.close();
    if (process.env.STACK_BENCH_LOCAL_AUTH_EVIDENCE) {
      writeFileSync(process.env.STACK_BENCH_LOCAL_AUTH_EVIDENCE, JSON.stringify(results, null, 2));
    }
  }
});
