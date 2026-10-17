import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// A denial caused by self-protection or last-administrator protection is not
// proof that the server checked the caller's current administrator role.
test('role authorization probes distinguish access control from account safeguards', async t => {
  const browser = await chromium.launch({ headless: true });
  let mode = 'correct';
  const roles = new Map<string, string>();
  const sessions = new Map<string, { user: string; roleAtLogin: string }>();
  const users = ['admin', 'staff', 'staff2'];
  const writes: { actor: string; target: string; role: string; before: string; status: number; reason: string }[] = [];
  const reads: Record<string, string>[] = [];
  const evidence: unknown[] = [];
  const server = createServer(async (request, response) => {
    const path = new URL(request.url!, 'http://fixture.test').pathname;
    const session = sessions.get((request.headers.authorization ?? '').replace('Bearer ', ''));
    if (path === '/login') {
      let text = ''; for await (const chunk of request) text += String(chunk);
      const { username } = JSON.parse(text) as { username: string };
      const token = `session-${sessions.size + 1}`;
      sessions.set(token, { user: username, roleAtLogin: roles.get(username)! });
      response.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify({ token, username })); return;
    }
    if (path === '/state') {
      reads.push(Object.fromEntries(roles));
      response.writeHead(200, { 'Content-Type': 'application/json' }).end(JSON.stringify(Object.fromEntries(roles))); return;
    }
    if (path.startsWith('/api/staff/') && request.method === 'PUT') {
      let text = ''; for await (const chunk of request) text += String(chunk);
      const target = users[Number(path.split('/')[3]) - 1]!;
      const { role } = JSON.parse(text) as { role: string };
      const actor = session?.user ?? '';
      const before = roles.get(target)!;
      const currentRole = roles.get(actor);
      const authorized = mode === 'no-admin-check-self-guard'
        || (mode === 'stale-session-last-admin' ? session?.roleAtLogin : currentRole) === 'admin';
      const reason = !session ? 'no-session'
        : target === actor ? 'self-change'
        : before === 'admin' && role !== 'admin' && [...roles.values()].filter(value => value === 'admin').length === 1
          ? 'last-administrator'
          : !authorized ? 'not-administrator' : 'accepted';
      const status = reason === 'accepted' ? 200 : 403;
      if (mode === 'delayed-save') await new Promise(resolve => setTimeout(resolve, 250));
      if (status === 200 && mode !== 'false-save-receipt') roles.set(target, role);
      writes.push({ actor, target, role, before, status, reason });
      response.writeHead(status, { 'Content-Type': 'application/json' })
        .end(JSON.stringify(status === 200 ? { ok: true } : { error: reason })); return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' }).end(`<!doctype html><main></main><script>
      const main = document.querySelector('main');
      const session = JSON.parse(sessionStorage.getItem('session') || 'null');
      window.getSessionToken = () => session?.token ?? null;
      if (!session) {
        main.innerHTML = '<form><input id="signin-username"><input id="signin-password"><button id="signin-submit">Sign in</button></form>';
        main.querySelector('form').onsubmit = async event => {
          event.preventDefault();
          const response = await fetch('/login', {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({username:document.querySelector('#signin-username').value})});
          sessionStorage.setItem('session', JSON.stringify(await response.json())); location.reload();
        };
      } else {
        async function render() {
          const roles = await (await fetch('/state')).json();
          main.innerHTML = '<span id="current-user">'+session.username+'</span><button id="admin-link">Admin</button><button id="staff-link">Staff</button><section id="staff-area"></section>';
          for (const [index, username] of ${JSON.stringify(users)}.entries()) {
            const row = document.createElement('form'); row.id = 'staff-role-account-'+encodeURIComponent(username);
            row.dataset.role = 'staff-role-row'; row.dataset.accountId = String(index+1); row.dataset.submitState = 'idle';
            row.innerHTML = username+'<select data-role="staff-role-select"><option>staff</option><option>inventory</option><option>admin</option></select><button data-role="staff-role-save">Save</button>';
            row.querySelector('select').value = roles[username];
            row.onsubmit = async event => {
              event.preventDefault();
              row.dataset.submitState = 'pending';
              const response = await fetch('/api/staff/'+(index+1)+'/role', {method:'PUT',headers:{'Content-Type':'application/json',Authorization:'Bearer '+session.token},body:JSON.stringify({role:row.querySelector('select').value})});
              row.dataset.submitState = response.ok ? 'succeeded' : 'failed';
            };
            main.append(row);
          }
        }
        render();
      }
      </script>`);
  });
  try {
    await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
    const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
    for (const fixture of [
      { check: '621b', mode: 'correct', status: 'passed' },
      { check: '621b', mode: 'no-admin-check-self-guard', status: 'failed' },
      { check: '621d', mode: 'correct', status: 'passed' },
      { check: '621d', mode: 'stale-session-last-admin', status: 'failed' },
      { check: '621a', mode: 'correct', status: 'passed' },
      { check: '621a', mode: 'delayed-save', status: 'passed' },
      { check: '621a', mode: 'false-save-receipt', status: 'failed' },
    ]) await t.test(`${fixture.check}: ${fixture.mode}`, async () => {
      mode = fixture.mode; writes.length = 0; reads.length = 0; sessions.clear(); roles.clear();
      for (const user of users) roles.set(user, user === 'admin' ? 'admin' : 'staff');
      const source = join(STACK_BENCH_ROOT, 'tracks/ecommerce/scenarios/progression-staff-roles.json');
      const definition = JSON.parse(readFileSync(source, 'utf8'));
      const selected = definition.features[0];
      selected.criteria = selected.criteria.filter((criterion: { id: string }) => criterion.id === fixture.check);
      if (fixture.check === '621a') {
        // This browser fixture measures the first persisted read, not database restart.
        const restart = selected.criteria[0].steps.findIndex((step: { do: string }) => step.do === 'restartBackend');
        assert.ok(restart > 0);
        selected.criteria[0].steps = selected.criteria[0].steps.slice(0, restart);
      }
      for (const step of [...selected.setup, ...selected.criteria[0].steps]) {
        if ('within' in step || ['click', 'fill', 'expect'].includes(step.do)) step.within = 1000;
        if ('settleMs' in step) step.settleMs = 0;
      }
      const feature = compileScenarioDefinition(definition, { source }).features[0]!;
      const grade = await gradeFeature(browser, feature,
        { url, level: definition.level, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: `role-authority-${fixture.check}-${mode}`, roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      const row = { ...fixture, grade, writes: [...writes], reads: [...reads], roles: Object.fromEntries(roles) };
      evidence.push(row);
      assert.equal(grade.criteria[0]!.evidence.status, fixture.status, JSON.stringify(row));
      if (fixture.mode === 'no-admin-check-self-guard' || fixture.mode === 'stale-session-last-admin') {
        assert.ok(writes.some(write => write.actor === 'staff' && write.target !== 'staff' && write.status === 200),
          'the deliberate authorization defect must reach another account');
      }
    });
  } finally {
    await browser.close(); server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_ROLE_AUTHORIZATION_EVIDENCE) {
      writeFileSync(process.env.STACK_BENCH_ROLE_AUTHORIZATION_EVIDENCE, JSON.stringify({
        rerun: 'STACK_BENCH_ROLE_AUTHORIZATION_EVIDENCE=<file> node --test dist/tests/staff-role-authorization.integration.js', evidence,
      }, null, 2));
    }
  }
});
