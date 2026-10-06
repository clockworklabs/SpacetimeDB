import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { writeFileSync } from 'node:fs';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

// Integration gap, specified before scoping interception: credential patches must
// reach the socket in setup, ordinary criteria, replacement pages and fresh actors.
// A server that trusts the supplied role must fail; refusing elevation must pass.
test('selected authentication probes survive browser lifecycle and expose privilege escalation', async () => {
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const server = createServer((_request, response) => {
    response.setHeader('Content-Type', 'text/html');
    response.end(`<input id="signup-username"><input id="signup-password"><button id="signup-submit">Join</button>
      <span id="current-user" hidden></span><span id="authority"></span><script>
      const socket = new WebSocket(location.origin.replace('http:', 'ws:') + '/api/1.0.0/sync');
      const ready = new Promise(resolve => socket.onopen = resolve);
      document.querySelector('button').onclick = async () => { await ready;
        socket.send(JSON.stringify({type:'Action',requestId:1,udfPath:'auth:signup',args:[{
          username:document.querySelector('#signup-username').value,
          password:document.querySelector('#signup-password').value}]})); };
      socket.onmessage = event => { const reply = JSON.parse(event.data);
        document.querySelector('#authority').textContent = reply.result.role;
        document.querySelector('#current-user').textContent = reply.result.username;
        document.querySelector('#current-user').hidden = false; };
      </script>`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const sockets = new wsServer({ server });
  let defective = false;
  const received: { role?: string }[] = [];
  sockets.on('connection', (socket: { on(event: string, fn: (data: Buffer) => void): void; send(data: string): void }) => {
    socket.on('message', data => {
      const message = JSON.parse(String(data)), account = message.args[0];
      received.push(account);
      socket.send(JSON.stringify({ type: 'ActionResponse', requestId: message.requestId, success: true,
        result: { username: account.username, role: defective ? account.role ?? 'user' : 'user' } }));
    });
  });
  const browser = await chromium.launch({ headless: true });
  const evidence: unknown[] = [];
  try {
    for (const phase of ['setup', 'criterion', 'replacement', 'fresh', 'race'] as const) for (defective of [false, true]) {
      received.length = 0;
      const actor = phase === 'fresh' ? 'claimant-fresh' : 'claimant';
      const signup = { do: 'signUp', actor, name: 'claimant', password: 'fixture-password',
        requestPatch: { fields: { role: 'admin' } } };
      const prepare = phase === 'replacement'
        ? [{ do: 'closeClient', actor }, { do: 'openClient', actor, settleMs: 0 }]
        : phase === 'fresh' ? [{ do: 'freshClient', actor: 'claimant' }] : [];
      const submit = phase === 'race' ? [{ do: 'race', settleMs: 0,
        branches: [[signup], [{ do: 'expect', actor, testid: 'signup-username' }]] }] : [signup];
      const feature = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1, features: [{
        id: 1, name: 'authentication boundary', actors: ['claimant'], setup: phase === 'setup' ? [signup] : [],
        criteria: [{ id: 'authority', desc: 'signup cannot choose administrator authority', points: 1,
          steps: [...prepare, ...(phase === 'setup' ? [] : submit),
            { do: 'expect', actor, testid: 'authority', contains: 'user', within: 300 }] }],
      }] }).features[0]!;
      const result = await gradeFeature(browser, feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'auth-interception', roomName: name => name, url, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      evidence.push({ phase, defective, result });
      assert.equal(result.setupEvidence.status, 'passed');
      assert.deepEqual(received.map(account => account.role), ['admin'], 'the native server must receive the patch once');
      assert.equal(result.criteria[0]!.evidence.status, defective ? 'failed' : 'passed', JSON.stringify(result));
    }
  } finally {
    await browser.close(); sockets.close();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_AUTH_EVIDENCE) writeFileSync(process.env.STACK_BENCH_AUTH_EVIDENCE,
      JSON.stringify({ rerun: 'STACK_BENCH_AUTH_EVIDENCE=<file> node --test dist/tests/auth-interception.integration.js', evidence }, null, 2));
  }
});
