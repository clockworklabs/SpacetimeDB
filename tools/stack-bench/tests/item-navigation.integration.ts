import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { stableElementSelector } from '../src/actions/element-selector.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('filter setup supports automatic updates and an optional Apply control', async () => {
  const scenario = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'tracks/ecommerce/scenarios/progression-faceted-filters.json'), 'utf8'));
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const explicitApply of [false, true]) {
      // The setup zeroes stock directly, then reloads before filtering.
      await page.route('http://filters.test/', route => route.fulfill({ contentType: 'text/html', body: `
        <select id="category-filter"><option>All</option><option>Home</option></select>
        <input id="minimum-price"><input id="maximum-price">
        ${explicitApply ? '<button id="filter-apply">Apply</button>' : ''}
        <output id="search-results">Unfiltered</output>
        <script>
          function apply() {
            document.querySelector('output').textContent = [
              document.querySelector('select').value,
              document.querySelector('#minimum-price').value,
              document.querySelector('#maximum-price').value,
            ].join('|');
          }
          if (${explicitApply}) document.querySelector('button').onclick = apply;
          else for (const event of ['input', 'change']) document.body.addEventListener(event, apply);
        </script>` }));
      await page.goto('http://filters.test/');
      const actor = { page, loc: (id: string) => page.locator(stableElementSelector(id)) };
      const writes: unknown[] = [];
      const sleep = async () => {};
      for (const step of scenario.features[0].setup) {
        const result = await executeAction(ACTION_REGISTRY, step.do, step, { capabilities: {
          actors: { get: () => actor },
          'browser-interaction': { defaultWithin: 1000, expand: (value: string) => value,
            testId: stableElementSelector, sleep },
          clock: { sleep },
          'database-write': { setStock: async (input: unknown) => { writes.push(input); return input; } },
        } });
        assert.equal(result.status, 'passed', result.summary ?? undefined);
      }
      assert.equal(writes.length, 2);
      assert.equal(await page.locator('#search-results').innerText(), 'Home|50|200');
      await page.unroute('http://filters.test/');
    }
  } finally { await browser.close(); }
});

test('openItem uses the declared name hook on links, buttons, and clickable cards', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const control of ['button', 'a', 'card']) {
      await page.setContent(`
        <article data-role="item-card">Other item<button>View</button></article>
        <article data-role="item-card" id="selected-card">
          <${control === 'card' ? 'h3' : control} data-role="item-name">Desk Lamp</${control === 'card' ? 'h3' : control}>
          <button id="buy">Buy now</button>
        </article>
        <section id="item-detail" hidden></section>
        <script>
          document.querySelector('#buy').onclick = () => { throw Error('must not buy'); };
          document.querySelector(${JSON.stringify(control === 'card'
            ? '#selected-card' : '[data-role="item-name"]')}).onclick = event => {
              event.preventDefault();
              const detail = document.querySelector('#item-detail');
              detail.textContent = 'Desk Lamp details'; detail.hidden = false;
            };
        </script>`);
      const actor = { page, loc: (id: string, options?: { contains?: string }) => {
        const locator = page.locator(stableElementSelector(id));
        return options?.contains ? locator.filter({ hasText: options.contains }) : locator;
      } };
      const result = await executeAction(ACTION_REGISTRY, 'openItem',
        { do: 'openItem', actor: 'visitor', item: 'Desk Lamp' }, { capabilities: {
          actors: { get: () => actor },
          'browser-interaction': { defaultWithin: 1000, expand: (value: string) => value,
            testId: stableElementSelector },
        } });
      assert.equal(result.status, 'passed', result.summary ?? JSON.stringify(result));
      assert.equal(await page.locator('#item-detail').innerText(), 'Desk Lamp details');
    }
  } finally { await browser.close(); }
});

test('openItem preserves inline variants and otherwise requires successful detail navigation', async () => {
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    for (const placement of ['inline', 'detail', 'broken']) {
      await page.setContent(`
        <article data-role="item-card">Other item<span data-role="item-variant">Other variant</span></article>
        <article data-role="item-card">
          <button data-role="item-name">Travel Mug</button>
          <span data-role="item-variant" ${placement === 'inline' ? '' : 'hidden'}>Black</span>
        </article>
        <section id="item-detail" hidden><span data-role="item-variant">Black</span></section>
        <script>
          document.querySelector('[data-role="item-name"]').onclick = () => {
            document.body.dataset.clicked = 'true';
            if (${JSON.stringify(placement)} === 'detail') document.querySelector('#item-detail').hidden = false;
          };
        </script>`);
      const actor = { page, loc: (id: string, options?: { contains?: string }) => {
        const locator = page.locator(stableElementSelector(id));
        return options?.contains ? locator.filter({ hasText: options.contains }) : locator;
      } };
      const result = await executeAction(ACTION_REGISTRY, 'openItem', {
        do: 'openItem', actor: 'visitor', item: 'Travel Mug', unlessVisible: 'item-variant', within: 1000,
      }, { capabilities: {
        actors: { get: () => actor },
        'browser-interaction': { defaultWithin: 1000, expand: (value: string) => value,
          testId: stableElementSelector },
      } });
      assert.equal(result.status, placement === 'broken' ? 'failed' : 'passed', result.summary ?? undefined);
      assert.equal(await page.locator('body').getAttribute('data-clicked'), placement === 'inline' ? null : 'true');
      if (placement === 'detail') assert.equal(await page.locator('#item-detail').isVisible(), true);
    }
  } finally { await browser.close(); }
});
