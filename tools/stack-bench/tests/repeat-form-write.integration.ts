import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { mkdirSync, writeFileSync } from 'node:fs';
import test from 'node:test';
import { chromium } from 'playwright';
import { Actor } from '../grader/grade.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { classifyResponseContract } from '../src/actions/named-action-runtime.js';

// Failure cases, before implementation: partial/duplicate matches, unconfirmed
// responses, non-JSON bodies, foreign origins and absent capture use the form.
// Rejected or lost replay responses MUST NOT trigger another form submission.
// JSON quoting, current cookies and untouched business fields must survive replay.
// Two UI writes must differ only in the name. Generated IDs/nonces disable replay.
test('repeat form write selects UI before sending, never after an uncertain write', async () => {
  let mode = '', calls = 0;
  const rows: unknown[] = [], observations: unknown[] = [];
  const server = createServer(async (req, res) => {
    if (req.url === '/api/products') {
      const parts = []; for await (const part of req) parts.push(part);
      const body = JSON.parse(Buffer.concat(parts).toString()); calls++;
      assert.equal(req.headers.cookie, 'session=current');
      if (body.name.startsWith('New') && mode === 'refused') { res.writeHead(403).end(); return; }
      rows.push(body);
      if (mode === 'lost' && body.name.startsWith('New')) { res.destroy(); return; }
      res.setHeader('Content-Type', 'application/json'); res.end(mode.startsWith('convex-')
        ? JSON.stringify({ status: 'success', value: null }) : '{}');
    } else {
      res.setHeader('Content-Type', 'text/html');
      res.setHeader('Set-Cookie', 'session=current; Path=/');
      res.end(`<input data-role="name"><button data-role="save">Save</button><script>
        window.formSubmits=0;
        document.querySelector('button').onclick=async()=>{window.formSubmits++;
          await fetch('/api/products',{method:'POST',headers:{'Content-Type':'application/json',
            ...${JSON.stringify(mode)}.startsWith('convex-')?{Authorization:'Bearer caller-session'}:{}},
          body:JSON.stringify({name:document.querySelector('input').value,price:1.25,category:'Volume',
            ...${JSON.stringify(mode)}==='changing-argument'?{nonce:window.formSubmits}:{}})});};
      </script>`);
    }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch();
  let result = 'failed';
  try {
    for (mode of ['accepted', 'convex-http', 'convex-query', 'missing', 'partial', 'duplicate', 'duplicate-value', 'non-json', 'unconfirmed', 'foreign', 'changing-argument', 'refused', 'lost']) {
      calls = 0; rows.length = 0;
      const context = await browser.newContext();
      try {
        const page = await context.newPage(), actor = new Actor('admin', page, context);
        await actor.ready; await page.goto(url);
        for (const name of ['Original product', 'Control product']) {
          await page.locator('input').fill(name);
          const finished = page.waitForEvent('requestfinished', req => req.url().endsWith('/api/products'));
          await page.locator('button').click(); await finished;
        }
        // Let the actor's complete-response observer finish before selection.
        await page.waitForTimeout(50);
        if (mode === 'missing') actor.writes.length = 0;
        if (mode === 'duplicate') actor.writes.push({ ...actor.writes[0]! });
        if (mode === 'partial') actor.writes[0]!.body!.name = 'Original product plus';
        if (mode === 'duplicate-value') actor.writes[0]!.body!.category = 'Original product';
        if (mode === 'non-json') actor.writes[0]!.headers['content-type'] = 'application/x-www-form-urlencoded';
        if (mode === 'unconfirmed') Object.assign(actor.writes[0]!, { confirmed: false });
        if (mode === 'foreign') actor.writes[0]!.url = 'https://foreign.invalid/api/products';
        const interaction = { defaultWithin: 1000, expand: (s: string) => s, sleep: async () => {} };
        const outcome = await executeAction(ACTION_REGISTRY, 'repeatFormWrite', {
          do: 'repeatFormWrite', actor: 'admin', match: 'Original product', replacement: 'New "product" $1',
          control: 'Control product',
          fields: [{ testid: 'name', text: 'New "product" $1' }], submit: 'save',
        }, { capabilities: { actors: { get: () => actor }, 'browser-interaction': interaction,
          'transport-observation': interaction, 'named-actions': { fetch,
            classifyResponse: (request: { url: string }, response: { status: number; text: string }) =>
              classifyResponseContract({ ...request, responseContract: mode === 'convex-http' ? 'convex-mutation'
                : mode === 'convex-query' ? 'convex-query' : 'http' }, response),
          } } });
        await page.waitForTimeout(50);
        const replay = ['accepted', 'convex-http', 'refused', 'lost'].includes(mode);
        assert.equal(await page.evaluate(() => (window as unknown as { formSubmits: number }).formSubmits), replay ? 2 : 3, mode);
        assert.equal(calls, 3, `${mode}: a replay must never retry`);
        assert.equal(outcome.status, mode === 'refused' ? 'failed' : mode === 'lost' ? 'inconclusive' : 'passed', JSON.stringify(outcome));
        assert.deepEqual(rows, [
          { name: 'Original product', price: 1.25, category: 'Volume', ...mode === 'changing-argument' ? { nonce: 1 } : {} },
          { name: 'Control product', price: 1.25, category: 'Volume', ...mode === 'changing-argument' ? { nonce: 2 } : {} },
          ...mode === 'refused' ? [] : [{ name: 'New "product" $1', price: 1.25, category: 'Volume', ...mode === 'changing-argument' ? { nonce: 3 } : {} }],
        ]);
        observations.push({ mode, outcome, requests: calls });
      } finally { await context.close(); }
    }
    result = 'passed';
  } finally {
    await browser.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_REPLAY_EVIDENCE) {
      mkdirSync(process.env.STACK_BENCH_REPLAY_EVIDENCE, { recursive: true });
      writeFileSync(`${process.env.STACK_BENCH_REPLAY_EVIDENCE}/form-receipt.json`, JSON.stringify({ result, observations,
        rerun: 'node --test dist/tests/repeat-form-write.integration.js' }, null, 2));
    }
  }
});
