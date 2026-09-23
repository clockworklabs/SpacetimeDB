import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { mkdirSync, writeFileSync } from 'node:fs';
import test from 'node:test';
import { chromium } from 'playwright';
import { Actor } from '../grader/grade.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { convexFunctionRequest } from '../src/stacks/backends/convex-protocol.js';

// Before implementation: reject unconfirmed, wrong-ID, rejected, ambiguous,
// query-only and disconnected captures. Change one exact argument only; preserve
// authentication and all other fields. Never turn a lost reply into a refusal.
test('native UI mutation replay uses a confirmed request and preserves its business arguments', async () => {
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const rows: Record<string, unknown>[] = [];
  let mode = 'accepted';
  const server = createServer(async (req, res) => {
    if (req.url === '/api/mutation') {
      const chunks = []; for await (const part of req) chunks.push(part);
      const body = JSON.parse(Buffer.concat(chunks).toString());
      assert.equal(body.path, mode === 'other-module' ? 'catalog/products:create' : 'api:create_product');
      if (mode === 'bearer') assert.equal(req.headers.authorization, 'Bearer caller-session');
      else assert.equal(body.args.token, 'caller-session');
      if (mode === 'lost-reply') { rows.push(body.args); res.destroy(); return; }
      const accepted = mode !== 'replay-refused';
      if (accepted) rows.push(body.args);
      res.end(JSON.stringify(accepted ? { status: 'success', value: null }
        : { status: 'error', errorMessage: 'refused', errorData: null }));
    } else res.end('<body>Product form</body>');
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const sockets = new wsServer({ server });
  sockets.on('connection', (socket: { on(event: string, fn: (data: Buffer) => void): void; send(data: string): void; close(): void }) => {
    socket.on('message', data => {
      const body = JSON.parse(String(data));
      if (mode === 'disconnect') { socket.close(); return; }
      socket.send(JSON.stringify({ type: 'MutationResponse', requestId: body.requestId + 50, success: true }));
      if (mode === 'wrong-id' || mode === 'query') return;
      socket.send(JSON.stringify({ type: 'MutationResponse', requestId: body.requestId, success: mode !== 'rejected' }));
    });
  });
  const browser = await chromium.launch({ headless: true });
  const observations: unknown[] = [];
  let result = 'failed';
  try {
    for (mode of ['accepted', 'other-module', 'bearer', 'foreign-origin', 'encoded-value', 'rejected', 'wrong-id', 'ambiguous', 'duplicate-value', 'query', 'disconnect', 'signed-out', 'replay-refused', 'lost-reply']) {
      rows.length = 0;
      const context = await browser.newContext();
      try {
        const page = await context.newPage(), actor = new Actor('admin', page, context);
        await actor.ready; await page.goto(url);
        await page.evaluate(async ({ url, mode }) => {
          Object.assign(window, { getSessionToken: () => mode === 'signed-out' ? null : 'caller-session' });
          const socket = new WebSocket(url.replace('http:', 'ws:').replace('127.0.0.1', mode === 'foreign-origin' ? 'localhost' : '127.0.0.1') + '/api/1.0.0/sync');
          Object.assign(window, { replayTestSocket: socket });
          await new Promise(resolve => socket.addEventListener('open', resolve, { once: true }));
          if (mode === 'bearer') socket.send(JSON.stringify({ type: 'Authenticate', tokenType: 'User', value: 'caller-session' }));
          const args = { name: 'Original product', category: mode === 'duplicate-value' ? 'Original product' : 'Volume',
            price: mode === 'encoded-value' ? { $integer: 'AQAAAAAAAAA=' } : 1.25, variants: ['Standard'],
            ...(mode === 'bearer' ? {} : { token: 'caller-session' }) };
          for (let n = 0; n < (mode === 'ambiguous' ? 2 : 1); n++) socket.send(JSON.stringify({
            type: mode === 'query' ? 'ModifyQuerySet' : 'Mutation', requestId: n,
            udfPath: mode === 'other-module' ? 'catalog/products:create' : 'api:create_product', args: [args],
          }));
        }, { url, mode });
        // The fixture intentionally emits unmatched responses and disconnects.
        await page.waitForTimeout(100);
        const capabilities = { actors: { get: () => actor }, 'transport-observation': {
          defaultWithin: 1000, expand: (s: string) => s, sleep: async () => {},
          verification: { verified: () => {}, unverified: () => {} },
        }, 'named-actions': {
          request: (action: { reducer: string }, input: { values: Record<string, unknown> }) => ({
            ...convexFunctionRequest({ deploymentUrl: url, kind: 'mutation', path: action.reducer.includes(':') ? action.reducer : `api:${action.reducer}`, args: input.values }),
            responseContract: 'convex-mutation',
          }),
          fetch,
        } };
        const replay = await executeAction(ACTION_REGISTRY, 'replayAs', {
          do: 'replayAs', actor: 'admin', from: 'admin', match: 'Original product',
          swap: { find: 'Original product', with: 'New "product" $1' }, settleMs: 0,
        }, { capabilities });
        const accepted = ['accepted', 'other-module', 'bearer'].includes(mode);
        const supported = accepted || ['replay-refused', 'lost-reply'].includes(mode);
        assert.equal(replay.status, supported ? 'passed' : 'inconclusive', `${mode}: ${JSON.stringify(replay)}`);
        if (supported) {
          const outcome = await executeAction(ACTION_REGISTRY, 'expectReplayCompleted', {
            do: 'expectReplayCompleted', actor: 'admin', requireAccepted: true,
          }, { capabilities });
          assert.equal(outcome.status, accepted ? 'passed' : mode === 'lost-reply' ? 'inconclusive' : 'failed');
        }
        assert.deepEqual(rows, accepted || mode === 'lost-reply' ? [{ name: 'New "product" $1', category: 'Volume',
          price: 1.25, variants: ['Standard'], ...(mode === 'bearer' ? {} : { token: 'caller-session' }) }] : []);
        observations.push({ mode, status: replay.status, writes: rows.length });
      } finally { await context.close(); }
    }
    result = 'passed';
  } finally {
    await browser.close(); sockets.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    const out = process.env.STACK_BENCH_REPLAY_EVIDENCE;
    if (out) {
      mkdirSync(out, { recursive: true });
      writeFileSync(`${out}/receipt.json`, JSON.stringify({ result, observations,
        rerun: 'STACK_BENCH_REPLAY_EVIDENCE=<directory> node --test dist/tests/convex-captured-replay.integration.js' }, null, 2));
    }
  }
});
