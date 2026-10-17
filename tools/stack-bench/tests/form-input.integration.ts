import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// Failure cases, before the change: label/value disagreement, absent choices,
// disabled controls, delayed options, and wrong date normalization. The real
// browser must dispatch the normal input event; direct DOM assignment is not a fill.
test('form entry selects real choices and preserves refusal and delayed-readiness behavior', async () => {
  const browser = await chromium.launch({ headless: true });
  const cases = [
    { id: 'label', html: '<select><option value="daily">Daily</option><option value="weekly">Weekly</option></select>', text: 'Weekly', value: 'weekly', status: 'passed' },
    { id: 'value', html: '<select><option value="daily">Daily</option><option value="weekly">Weekly</option></select>', text: 'weekly', value: 'weekly', status: 'passed' },
    { id: 'ambiguous-native-choice', html: '<select><option value="a">Weekly</option><option value="Weekly">Other</option></select>', text: 'Weekly', value: 'a', status: 'passed' },
    { id: 'delayed', html: '<select></select><script>setTimeout(()=>document.querySelector("select").add(new Option("Weekly","weekly")),100)</script>', text: 'weekly', value: 'weekly', status: 'passed' },
    { id: 'missing', html: '<select><option value="daily">Daily</option></select>', text: 'Weekly', status: 'failed', finding: 'choice-missing' },
    { id: 'disabled', html: '<select disabled><option value="daily">Daily</option><option value="weekly">Weekly</option></select>', text: 'Weekly', status: 'failed', finding: 'page-timeout' },
    { id: 'date', html: '<input type="date">', text: '2026-09-22T14:00', value: '2026-09-22', status: 'passed' },
    { id: 'datetime', html: '<input type="datetime-local">', text: '2026-09-22', value: '2026-09-22T00:00', status: 'passed' },
    { id: 'text', html: '<input>', text: 'Volume Item 0999', value: 'Volume Item 0999', status: 'passed' },
  ];
  const observations: unknown[] = [];
  let result = 'failed';
  try {
    const page = await browser.newPage();
    page.setDefaultTimeout(400);
    for (const sample of cases) {
      await page.setContent(sample.html);
      await page.locator('select,input').evaluate(element => {
        element.addEventListener('input', () => element.setAttribute('data-input-seen', 'yes'));
      });
      const actor = { page, loc: () => page.locator('select,input') };
      const started = performance.now();
      const action = await executeAction(ACTION_REGISTRY, 'fill', { do: 'fill', actor: 'editor', testid: 'field', text: sample.text }, {
        capabilities: { actors: { get: () => actor }, 'browser-interaction': {
          defaultWithin: 400, expand: (value: string) => value, sleep: async () => {},
        } },
      });
      const ms = performance.now() - started;
      const value = await page.locator('select,input').inputValue();
      observations.push({ id: sample.id, ms, status: action.status, finding: action.finding?.kind, value });
      assert.equal(action.status, sample.status, `${sample.id}: ${action.summary}`);
      if (sample.finding) assert.equal(action.finding?.kind, sample.finding);
      if (sample.value !== undefined) {
        assert.equal(value, sample.value, sample.id);
        assert.equal(await page.locator('select,input').getAttribute('data-input-seen'), 'yes');
      }
    }
    result = 'passed';
  } finally {
    await browser.close();
    const evidence = process.env.STACK_BENCH_FORM_EVIDENCE
      ?? join(STACK_BENCH_ROOT, 'local-notes', 'form-input-evidence');
    mkdirSync(evidence, { recursive: true });
    writeFileSync(join(evidence, 'receipt.json'), JSON.stringify({ result,
        command: 'node --test dist/tests/form-input.integration.js', cases, observations, browserClosed: true,
        executorSha256: createHash('sha256').update(readFileSync(join(STACK_BENCH_ROOT,
          'dist/src/actions/browser-action-executors.js'))).digest('hex'),
    }, null, 2));
  }
});
