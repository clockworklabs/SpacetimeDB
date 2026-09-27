import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// The declared submit-state receipt precedes a verification reload. A receipt is
// not proof of a saved value, and unrelated reads are not part of the save.
test('support save completes before verification reload without changing timed reloads', async () => {
  let mode = '', stored = '', writes = 0;
  let events: { event: string; at: number; value?: string }[] = [];
  const evidence: unknown[] = [];
  const server = createServer(async (request, response) => {
    if (request.url === '/poll') {
      events.push({ event: 'poll-start', at: Date.now() });
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.write(' '); // A legitimate long read stays open until the page closes.
      return;
    }
    if (request.method === 'POST' && request.url === '/save-ticket') {
      let body = ''; for await (const chunk of request) body += String(chunk);
      const value = (JSON.parse(body) as { assignee: string }).assignee;
      writes++;
      const firstOfTwo = mode === 'overlapping-writes' && writes === 1;
      events.push({ event: 'save-start', at: Date.now(), value });
      if (mode === 'refused') {
        events.push({ event: 'save-refused', at: Date.now() });
        response.writeHead(409, { 'Content-Type': 'application/json' });
        response.end('{"error":"refused"}'); return;
      }
      if (mode !== 'lying-success') {
        await new Promise(resolve => setTimeout(resolve, firstOfTwo ? 500 : 2000));
        if (!firstOfTwo) {
          stored = value;
          events.push({ event: 'save-committed', at: Date.now(), value });
        }
      }
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end('{}'); return;
    }
    if (request.url !== '/') { response.writeHead(404).end(); return; }
    events.push({ event: 'page-load', at: Date.now(), value: stored });
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<!doctype html><span id="current-user">staff</span>
      <div data-role="support-ticket" ${mode === 'missing-receipt' ? '' : `data-submit-state="${mode === 'stale-success' ? 'succeeded' : 'idle'}"`}>Missing item
      <select data-role="support-assignee"><option value="">Unassigned</option>
      <option value="staff" ${stored === 'staff' ? 'selected' : ''}>staff</option></select>
      <button data-role="support-update">Save</button><span id="save-result"></span></div>
      <script>
      fetch('/poll').catch(() => {});
      document.querySelector('button').onclick = async () => {
        const ticket = document.querySelector('[data-role="support-ticket"]');
        ${mode === 'missing-receipt' ? '' : "ticket.dataset.submitState = 'pending';"}
        ${mode === 'delayed-request-start' ? 'await new Promise(resolve => setTimeout(resolve, 100));' : ''}
        const save = () => fetch('/save-ticket', { method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ assignee: document.querySelector('select').value }) });
        const result = ${mode === 'overlapping-writes' ? '(await Promise.all([save(), save()]))[1]' : 'await save()'};
        ${mode === 'missing-receipt' ? '' : "ticket.dataset.submitState = result.ok ? 'succeeded' : 'failed';"}
        document.querySelector('#save-result').textContent = result.ok ? 'Saved' : 'Refused';
      };
      </script>`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  let result = 'failed';
  try {
    const original = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
      'tracks/ecommerce/scenarios/progression-support-triage.json'), 'utf8'));
    for (mode of ['delayed', 'delayed-request-start', 'overlapping-writes', 'stale-success',
      'refused', 'missing-receipt', 'lying-success', 'ordinary-timed-reload']) {
      stored = ''; writes = 0; events = [];
      const definition = structuredClone(original);
      const feature = definition.features[0];
      feature.actors = ['staff']; feature.setup = [];
      feature.criteria = feature.criteria.filter((criterion: { id: string }) => criterion.id === '611a');
      if (mode === 'ordinary-timed-reload') {
        feature.criteria[0].steps = feature.criteria[0].steps
          .filter((step: { attribute?: string }) => step.attribute !== 'data-submit-state')
          .map((step: { do: string; testid?: string }) =>
          step.do === 'click' && step.testid === 'support-update'
            ? { do: 'click', actor: 'staff', testid: 'support-update',
              in: { testid: 'support-ticket', contains: 'Missing item' }, settleMs: 1500 }
            : step);
      }
      const compiled = compileScenarioDefinition(definition).features[0]!;
      const grade = await gradeFeature(browser, compiled,
        { url, level: 2, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'support-save-completion', roomName: name => name, url, actions: [],
          spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 4000 });
      evidence.push({ mode, grade, events: [...events], writes, stored, steps: feature.criteria[0].steps });
      assert.equal(grade.setupEvidence.status, 'passed', JSON.stringify(grade));
      const correct = ['delayed', 'delayed-request-start', 'overlapping-writes', 'stale-success'].includes(mode);
      assert.equal(writes, mode === 'overlapping-writes' ? 2 : 1, `${mode}: no harness write retry`);
      assert.equal(grade.criteria[0]!.evidence.status, correct ? 'passed' : 'failed',
        JSON.stringify({ mode, grade, events }));
      const reload = events.filter(event => event.event === 'page-load')[1];
      if (correct || mode === 'ordinary-timed-reload') {
        const commit = events.find(event => event.event === 'save-committed');
        assert.ok(reload && commit, JSON.stringify(events));
        assert.equal(reload.value, correct ? 'staff' : '', JSON.stringify(events));
        assert.equal(reload.at >= commit.at, correct, JSON.stringify(events));
      } else {
        assert.equal(stored, mode === 'missing-receipt' ? 'staff' : '', `${mode}: persisted state`);
        if (mode === 'refused' || mode === 'missing-receipt') {
          assert.equal(reload, undefined, `${mode}: no reload without the required success receipt`);
        } else {
          assert.ok(reload, 'a lying receipt must still face the persisted-state check');
          assert.equal(reload.value, '', 'the lying receipt must not manufacture stored state');
        }
      }
    }
    result = 'passed';
  } finally {
    await browser.close();
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_SUPPORT_SAVE_EVIDENCE) {
      const file = process.env.STACK_BENCH_SUPPORT_SAVE_EVIDENCE;
      mkdirSync(dirname(file), { recursive: true });
      writeFileSync(file, JSON.stringify({ result,
        rerun: 'node --test dist/tests/support-save.integration.js', evidence }, null, 2));
    }
  }
});

// A valid save may finish after the click returns. An ordinary persistence or
// access check must not reload first and then judge a stale page-load snapshot.
test('ordinary profile, notification and promotion saves finish before verification reload', async t => {
  type Event = { event: string; at: number; value?: string; method?: string; body?: string; status?: number };
  let mode = '', kind = '', stored = '', events: Event[] = [];
  const evidence: unknown[] = [];
  const server = createServer(async (request, response) => {
    if (request.method === 'POST' && request.url === '/save') {
      let body = ''; for await (const chunk of request) body += String(chunk);
      const value = (JSON.parse(body) as { value: string }).value;
      events.push({ event: 'save-start', at: Date.now(), value, method: request.method, body });
      response.on('close', () => events.push({ event: 'response-close', at: Date.now(), value }));
      response.on('finish', () => events.push({ event: 'response-finished', at: Date.now(), value, status: response.statusCode }));
      if (mode === 'refused') {
        events.push({ event: 'save-refused', at: Date.now() });
        response.writeHead(409, { 'Content-Type': 'application/json' }); response.end('{}'); return;
      }
      if (['delayed', 'delayed-request-start', 'stale-success', 'close-after-save'].includes(mode)) {
        await new Promise(resolve => setTimeout(resolve, 2000));
      }
      // The durable operation is valid even if the harness has closed its page.
      if (mode !== 'lying-success') {
        stored = value; events.push({ event: 'save-committed', at: Date.now(), value });
      }
      response.writeHead(200, { 'Content-Type': 'application/json' }); response.end(JSON.stringify({ saved: value })); return;
    }
    if (request.url !== '/') { response.writeHead(404).end(); return; }
    events.push({ event: 'page-load', at: Date.now(), value: stored });
    response.writeHead(200, { 'Content-Type': 'text/html' });
    const profile = kind === 'profile', notification = kind === 'notification';
    const saveId = profile ? 'profile-save' : notification ? 'notification-save' : 'promotion-submit';
    const receiptId = profile ? 'profile-save-state' : 'notification-save-state';
    const username = profile ? 'profile-private-ownerordinary-save-completionf620'
      : notification ? 'notification-ownerordinary-save-completionf630' : 'staff';
    response.end(`<!doctype html><span id="current-user">${username}</span>
      <button id="catalog-link">Catalog</button><div id="item-list"></div>
      ${profile || notification ? `<span id="${receiptId}" ${mode === 'missing-receipt' ? ''
        : `data-submit-state="${mode === 'stale-success' ? 'succeeded' : 'idle'}"`}>Save status</span>` : ''}
      ${profile ? `<button id="profile-link">Profile</button><div id="editor"><input id="profile-name"><input id="profile-address">
        <button id="profile-save">Save</button><p id="profile-address-summary">${stored}</p></div>`
        : notification ? `<button id="notification-settings">Settings</button><div id="editor">
        <button id="notification-order" data-state="${stored === 'on' ? 'on' : 'off'}">Order notifications</button>
        <button id="notification-save">Save</button></div>`
        : `<button id="staff-link">Staff</button><button id="promotions-link">Promotions</button>
        <input id="promotion-code"><input id="promotion-discount"><input id="promotion-start" type="date">
        <input id="promotion-end" type="date"><input id="promotion-limit"><button id="promotion-submit">Save</button>
        <div id="rules">${stored ? `<div data-role="promotion-item">${stored}</div>` : ''}</div>`}
      <script>
        ${profile || notification ? `document.querySelector('#${profile ? 'profile-link' : 'notification-settings'}').onclick = () => {
          document.querySelector('#editor').hidden = false;
        };` : ''}
        ${notification ? `document.querySelector('#notification-order').onclick = event => {
          event.currentTarget.dataset.state = event.currentTarget.dataset.state === 'on' ? 'off' : 'on';
        };` : ''}
        document.querySelector('#${saveId}').onclick = async () => {
          const receipt = document.querySelector('#${receiptId}');
          ${mode === 'missing-receipt' ? '' : "if (receipt) receipt.dataset.submitState = 'pending';"}
          const value = ${notification ? "document.querySelector('#notification-order').dataset.state"
            : `document.querySelector('#${profile ? 'profile-address' : 'promotion-code'}').value`};
          ${mode === 'delayed-request-start' ? 'await new Promise(resolve => setTimeout(resolve, 100));' : ''}
          const response = await fetch('/save', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ value }) });
          const result = await response.json();
          ${mode === 'missing-receipt' ? '' : "if (receipt) receipt.dataset.submitState = response.ok ? 'succeeded' : 'failed';"}
          if (!response.ok) return;
          ${profile ? "document.querySelector('#profile-address-summary').textContent = result.saved;"
            : notification ? '' : "const row = document.createElement('div'); row.dataset.role = 'promotion-item'; row.textContent = result.saved; document.querySelector('#rules').append(row);"}
          ${mode === 'close-after-save' ? "document.querySelector('#editor').hidden = true;" : ''}
        };
      </script>`);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    for (kind of ['profile', 'notification', 'promotion']) for (mode of kind === 'promotion'
      ? ['immediate', 'delayed'] : ['immediate', 'delayed', 'delayed-request-start', 'stale-success',
        'close-after-save', 'refused', 'missing-receipt', 'lying-success']) {
      await t.test(`${kind}-${mode}`, async () => {
        stored = ''; events = [];
        const source = join(STACK_BENCH_ROOT, `tracks/ecommerce/scenarios/progression-${kind === 'profile'
          ? 'customer-profile' : kind === 'notification' ? 'notification-preferences' : 'promotion-rules'}.json`);
        const definition = JSON.parse(readFileSync(source, 'utf8'));
        const feature = definition.features[0], criterion = feature.criteria.find((item: { id: string }) =>
          item.id === (kind === 'notification' ? '630a' : '620b'));
        const actor = kind === 'profile' ? 'privateOwner' : kind === 'notification' ? 'owner' : 'staff';
        const steps = (kind === 'notification' ? [...feature.setup, ...criterion.steps] : criterion.steps) as
          { do: string; actor?: string; testid?: string; contains?: string }[];
        const first = steps.findIndex(step => kind === 'notification'
          ? step.do === 'click' && step.testid === 'notification-order'
          : step.do === 'fill' && step.testid === (kind === 'profile' ? 'profile-name' : 'promotion-code'));
        const reloadIndex = steps.findIndex((step, index) => index > first && step.do === 'reload');
        const last = steps.findIndex((step, index) => index > reloadIndex && step.do === 'expect'
          && step.testid === (kind === 'profile' ? 'profile-address-summary'
            : kind === 'notification' ? 'notification-order' : 'promotion-item'));
        assert(first >= 0 && reloadIndex > first && last > reloadIndex,
          'select the authored save, reload and persisted read; retain any completion receipt');
        feature.actors = [actor]; feature.setup = [];
        feature.criteria = [{ ...criterion, steps: steps.slice(first, last + 1) }];
        const compiled = compileScenarioDefinition(definition).features[0]!;
        const grade = await gradeFeature(browser, compiled,
          { url, level: 2, headed: false, selectedCheckKeys: [], nullControl: false },
          { runId: 'ordinary-save-completion', roomName: name => name, url, actions: [],
            spacetime: null, backend: 'postgres', nullControl: false, defaultWithin: 4000 });
        // Preserve evidence before the expected red assertion.
        evidence.push({ kind, mode, source, steps: feature.criteria[0].steps, grade, stored, events: [...events] });
        assert.equal(grade.setupEvidence.status, 'passed', JSON.stringify(grade));
        const commit = events.find(event => event.event === 'save-committed');
        const reload = events.filter(event => event.event === 'page-load')[1];
        assert.equal(events.filter(event => event.event === 'save-start').length, 1, 'no write retry');
        const correct = !['refused', 'missing-receipt', 'lying-success'].includes(mode);
        assert.equal(grade.criteria[0]!.evidence.status, correct ? 'passed' : 'failed',
          JSON.stringify({ kind, mode, grade, events }));
        if (correct) {
          assert(commit && reload, JSON.stringify(events));
          assert(stored, 'the valid app committed the submitted value');
          assert(reload.at >= commit.at, 'ordinary verification must follow the completed save');
        } else if (mode === 'refused' || mode === 'missing-receipt') {
          assert.equal(reload, undefined, 'no verification reload before a required success receipt');
        } else {
          assert(reload, 'a lying receipt must still face the persisted-state read');
          assert.equal(reload.value, '', 'a receipt must not manufacture durable state');
        }
      });
    }
  } finally {
    await browser.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_ORDINARY_SAVE_EVIDENCE) {
      mkdirSync(dirname(process.env.STACK_BENCH_ORDINARY_SAVE_EVIDENCE), { recursive: true });
      writeFileSync(process.env.STACK_BENCH_ORDINARY_SAVE_EVIDENCE, JSON.stringify({
        rerun: 'node --test --test-name-pattern="ordinary profile, notification and promotion" dist/tests/support-save.integration.js', evidence,
      }, null, 2));
    }
  }
});
