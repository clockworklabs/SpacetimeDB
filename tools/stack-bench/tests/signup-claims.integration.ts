import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import test from 'node:test';
import { chromium } from 'playwright';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { gradeFeature } from '../grader/grade.js';
import { classifyMutationResult, indexMutationReport } from '../src/evidence/mutation-analysis.js';

// Failure cases: a correct two-write registration must pass; a claim accepted by
// the second write must fail even if the first write is safe; refusing every
// registration must fail; an unreached second target must remain inconclusive.
test('signup claims grade each registration write through a fresh browser session', async t => {
  let mode = '', stock = 0, registrations = 0;
  const accounts = new Map<string, string>();
  const calls: { path: string; claim: unknown; status: number }[] = [];
  const evidence: unknown[] = [];
  let baselineGrade: Awaited<ReturnType<typeof gradeFeature>> | undefined;
  const server = createServer(async (req, res) => {
    const path = new URL(req.url!, 'http://fixture.test').pathname;
    let raw = ''; for await (const chunk of req) raw += String(chunk);
    const body = raw ? JSON.parse(raw) as Record<string, unknown> : {};
    const cookies = Object.fromEntries((req.headers.cookie ?? '').split(';').filter(Boolean)
      .map(item => item.trim().split('=').map(decodeURIComponent)));
    const send = (status: number, value: unknown) => {
      calls.push({ path, claim: body.role ?? null, status });
      res.writeHead(status, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(value));
    };
    if (path === '/profile') {
      registrations++;
      if (mode === 'reject-all' || mode === 'unreached' && registrations === 3) return send(403, { error: 'Registration refused' });
      res.setHeader('Set-Cookie', `pending=${encodeURIComponent(String(body.username))}; HttpOnly; Path=/`);
      return send(200, { ok: true });
    }
    if (path === '/finish') {
      assert(cookies.pending, 'registration must first send its ordinary profile request');
      accounts.set(cookies.pending, mode === 'unsafe-finalizer' && body.role === 'admin' ? 'admin' : 'customer');
      res.setHeader('Set-Cookie', `session=${encodeURIComponent(cookies.pending)}; HttpOnly; Path=/`);
      return send(200, { user: cookies.pending });
    }
    if (path === '/login') {
      const user = String(body.username);
      if (!accounts.has(user)) return send(403, { error: 'Unknown account' });
      res.setHeader('Set-Cookie', `session=${encodeURIComponent(user)}; HttpOnly; Path=/`);
      return send(200, { user });
    }
    if (path === '/restock') {
      const allowed = accounts.get(cookies.session ?? '') === 'admin';
      if (allowed) stock++;
      return send(allowed ? 200 : 403, allowed ? { ok: true } : { error: 'Administrator required' });
    }
    res.writeHead(200, { 'Content-Type': 'text/html' });
    res.end(`<!doctype html><span id="stock">${stock}</span><main></main><script>
      window.getSessionToken=()=>null;
      const main=document.querySelector('main');
      function show(user) { main.innerHTML='<span id="current-user"></span>'+(${JSON.stringify(mode)}==='missing-signout'?'':'<button id="signout">Sign out</button>'); document.querySelector('#current-user').textContent=user;
        const signout=document.querySelector('#signout'); if(signout)signout.onclick=()=>location.reload(); }
      async function post(path, value) {
        const r=await fetch(path,{method:'POST',...(value?{headers:{'Content-Type':'application/json'},body:JSON.stringify(value)}:{})});
        const body=await r.json(); if(!r.ok)throw new Error(body.error); return body;
      }
      main.innerHTML='<input id="signup-username"><input id="signup-password"><button id="signup-submit">Register</button><input id="signin-username"><input id="signin-password"><button id="signin-submit">Login</button>';
      const startup=${JSON.stringify(mode.startsWith('startup-'))}, deferred=${registrations > 0};
      let socket, connectDone;
      if(startup) {
        const button=document.querySelector('#signup-submit'); button.disabled=true;
        let opened, sent=false;
        const opening=new Promise(resolve=>opened=resolve);
        socket=new WebSocket(location.origin.replace('http:','ws:')+'/socket.io/?EIO=4&transport=websocket');
        socket.onmessage=event=>{
          if(String(event.data).startsWith('0')) { opened(); if(!deferred)socket.send('40'); }
          if(String(event.data).startsWith('40')) {
            button.disabled=false;
            if(!document.querySelector('#connection-ready'))main.insertAdjacentHTML('beforeend','<span id="connection-ready">Ready</span>');
            connectDone?.();
          }
        };
        document.querySelector('#signup-username').oninput=async()=>{
          if(deferred&&!sent) { sent=true; await opening; socket.send('40'); }
        };
      }
      document.querySelector('#signup-submit').onclick=async()=>{try{
        if(${JSON.stringify(mode)}==='client-reject-all')throw new Error('Registration refused');
        await post('/profile',{username:document.querySelector('#signup-username').value,password:document.querySelector('#signup-password').value});
        const created=await post('/finish');
        if(startup)await new Promise(resolve=>{connectDone=resolve;socket.send('41');socket.send('40'+JSON.stringify({user:created.user}));});
        show(created.user);
      }catch(e){main.insertAdjacentHTML('beforeend','<span id="auth-error">Registration refused</span>');}};
      document.querySelector('#signin-submit').onclick=async()=>{try{
        show((await post('/login',{username:document.querySelector('#signin-username').value,password:document.querySelector('#signin-password').value})).user);
      }catch(e){main.insertAdjacentHTML('beforeend','<span id="auth-error">Login refused</span>');}};
    </script>`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const sockets = new wsServer({ server });
  sockets.on('connection', (socket: { send(value: string): void; on(event: string, callback: (value: Buffer) => void): void }) => {
    socket.send('0'+JSON.stringify({sid: 'fixture', upgrades: [], pingInterval: 25000, pingTimeout: 20000}));
    socket.on('message', value => {
      const frame=String(value); if(!frame.startsWith('40'))return;
      const claims=frame.length>2 ? JSON.parse(frame.slice(2)) : {};
      if(mode==='startup-unsafe'&&claims.role==='admin'&&accounts.has(claims.user))accounts.set(claims.user,'admin');
      calls.push({path:'/socket-connect',claim:claims.role??null,status:200});
      socket.send('40'+JSON.stringify({sid:'namespace'}));
    });
  });
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (const [variant, expected] of [['correct', 'passed'], ['unsafe-finalizer', 'failed'],
      ['reject-all', 'failed'], ['client-reject-all', 'failed'], ['unreached', 'inconclusive'],
      ['missing-signout', 'failed'], ['startup-safe', 'passed'], ['startup-unsafe', 'failed']]) await t.test(variant!, async () => {
      mode = variant!; stock = 0; registrations = 0; calls.length = 0; accounts.clear(); accounts.set('admin', 'admin');
      const action = { id: 'restock', path: '/restock', method: 'POST', reducer: 'restock', args: [], params: [] };
      const definition = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1,
        name: 'Signup write observation', features: [{ id: 103, name: 'Signup claims', actors: ['admin', 'claimant'],
          setup: [{ do: 'signIn', actor: 'admin', name: 'admin', password: 'fixture-password', exact: true },
            ...(mode.startsWith('startup-') ? [{ do:'expect', actor:'claimant', testid:'connection-ready' }] : [])],
          criteria: [{ id: '103b', category: 'production', desc: 'Signup claims do not grant stock authority', points: 1,
            steps: [
              { do: 'callAction', actor: 'admin', action: 'restock', namedAction: action, settleMs: 0 },
              { do: 'expectActionOutcome', actor: 'admin', outcome: 'accepted' },
              { do: 'probeSignupClaims', actor: 'claimant', name: 'claimant', fields: { role: 'admin' }, branches: [[
                { do: 'callAction', actor: 'claimant', action: 'restock', namedAction: action, authentication: 'optional', settleMs: 0 },
                { do: 'reload', actor: 'admin', settleMs: 0 },
                { do: 'expectNumber', actor: 'admin', testid: 'stock', equals: 1, within: 200 },
                { do: 'expectActionOutcome', actor: 'claimant', outcome: 'application-refused', routeProvenBy: 'admin' },
              ]] },
            ] }] }] });
      const grade = await gradeFeature(browser, definition.features[0]!,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { url, runId: `signup-${variant}`, roomName: name => name, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 200 });
      evidence.push({ variant, grade, calls: [...calls], stock, registrations });
      assert.equal(grade.criteria[0]!.evidence.status, expected, JSON.stringify(evidence.at(-1)));
      if (variant === 'correct') baselineGrade = grade;
      if (variant === 'unsafe-finalizer' || variant === 'missing-signout') {
        assert(baselineGrade);
        const report = { features: [grade] };
        const classification = classifyMutationResult({ features: [baselineGrade] }, report, { targets: ['103:103b'] });
        assert.equal(indexMutationReport(report).criteria.get('103:103b')?.failedAction,
          variant === 'unsafe-finalizer' ? 'expectNumber' : 'click');
        assert.equal(classification.status, variant === 'unsafe-finalizer' ? 'CAUGHT' : 'CAUGHT_OFF_ASSERTION');
      }
      if (variant === 'unsafe-finalizer') {
        assert(calls.some(call => call.path === '/finish' && call.claim === 'admin'));
        assert.equal(stock, 2, 'the grader must observe the unauthorized stored effect');
      }
      if (variant === 'correct') assert.equal(registrations, 3, 'ordinary registration plus one fresh account per write');
      if (variant === 'startup-safe') assert.equal(registrations, 4, 'registration and reconnect remain three measured writes');
      if (variant === 'startup-unsafe') {
        assert(calls.some(call=>call.path==='/socket-connect'&&call.claim==='admin'));
        assert.equal(stock, 2, 'authority-bearing reconnect after signup must still be measured');
      }
    });
  } finally {
    await browser.close(); sockets.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_SIGNUP_CLAIMS_EVIDENCE) writeFileSync(process.env.STACK_BENCH_SIGNUP_CLAIMS_EVIDENCE,
      `${JSON.stringify({ schemaVersion: 1, modelCalls: 0, evidence }, null, 2)}\n`);
  }
});
