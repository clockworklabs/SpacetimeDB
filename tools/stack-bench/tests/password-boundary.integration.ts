import assert from 'node:assert/strict';
import { once } from 'node:events';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// Failure cases, defined before the action change: full-byte verification and
// explicit refusal above 72 bytes pass. Truncation, accepting every password,
// rejecting ordinary registration/login, and creating an account despite a
// registration refusal fail. Refusal must not leave a session after reload.
test('password scenario grades accepted bytes and safe unsupported-input refusal', async t => {
  let mode = '';
  const accounts = new Map<string, string>();
  const calls: { path: string; bytes: number; accepted: boolean }[] = [];
  const evidence: unknown[] = [];
  const server = createServer(async (req, res) => {
    const path = new URL(req.url!, 'http://fixture.test').pathname;
    if (path === '/signup' || path === '/signin') {
      let raw = ''; for await (const chunk of req) raw += String(chunk);
      const { username, password } = JSON.parse(raw) as { username: string; password: string };
      const bytes = Buffer.byteLength(password);
      const stored = mode === 'truncation' ? Buffer.from(password).subarray(0, 72).toString('hex') : password;
      const refusal = mode === 'reject-all' || bytes > 72 && mode.startsWith('refusal');
      let accepted: boolean;
      if (path === '/signup') {
        if (!refusal || mode === 'refusal-creates-account') accounts.set(username, stored);
        accepted = !refusal;
      } else accepted = mode !== 'reject-login' && accounts.has(username)
        && (mode === 'any-password' || accounts.get(username) === stored);
      calls.push({ path, bytes, accepted });
      if (accepted || mode === 'refusal-keeps-session' && bytes > 72) {
        res.setHeader('Set-Cookie', `session=${encodeURIComponent(username)}; HttpOnly; Path=/`);
      }
      res.writeHead(accepted ? 200 : 400, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify(accepted ? { user: username } : { error: 'Credentials refused' }));
      return;
    }
    const user = decodeURIComponent(/(?:^|;\s*)session=([^;]*)/.exec(req.headers.cookie ?? '')?.[1] ?? '');
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<!doctype html><main>
      <input id="signup-username"><input id="signup-password"><button id="signup-submit">Register</button>
      <input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button>
      </main><script>
      const main=document.querySelector('main');
      const show=user=>{main.insertAdjacentHTML('beforeend','<span id="current-user"></span>'); document.querySelector('#current-user').textContent=user;};
      if(${JSON.stringify(user)})show(${JSON.stringify(user)});
      for(const name of ['signup','signin'])document.querySelector('#'+name+'-submit').onclick=async()=>{
        document.querySelector('#auth-error')?.remove();
        const r=await fetch('/'+name,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({username:document.querySelector('#'+name+'-username').value,password:document.querySelector('#'+name+'-password').value})});
        const result=await r.json();
        if(r.ok)show(result.user); else main.insertAdjacentHTML('beforeend','<span id="auth-error">Credentials refused</span>');
      };
      </script>`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [variant, expected] of [
      ['full-password', 'passed'], ['refusal-safe', 'passed'],
      ['truncation', 'failed'], ['any-password', 'failed'],
      ['reject-all', 'failed'], ['reject-login', 'failed'],
      ['refusal-creates-account', 'failed'], ['refusal-keeps-session', 'failed'],
    ]) await t.test(variant!, async () => {
      mode = variant!; accounts.clear(); calls.length = 0;
      const scenario = compileScenarioDefinition(JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
        'tracks/ecommerce/scenarios/01-account-password.json'), 'utf8')));
      const grade = await gradeFeature(browser, scenario.features[0]!,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { url, runId: `password-${variant}`, roomName: name => name, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 200 });
      evidence.push({ variant, grade, calls: [...calls] });
      assert.equal(grade.criteria[0]!.evidence.status, expected, JSON.stringify(evidence.at(-1)));
      if (expected === 'passed') {
        assert(calls.some(call => call.path === '/signin' && call.bytes <= 72 && call.accepted),
          'ordinary account must work before safe refusal can earn credit');
        assert(calls.some(call => call.path === '/signup' && call.bytes === 74),
          'the truncation boundary must reach the application');
      }
    });
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_PASSWORD_BOUNDARY_EVIDENCE) {
      writeFileSync(process.env.STACK_BENCH_PASSWORD_BOUNDARY_EVIDENCE,
        `${JSON.stringify({ schemaVersion: 1, modelCalls: 0, evidence }, null, 2)}\n`);
    }
  }
});
