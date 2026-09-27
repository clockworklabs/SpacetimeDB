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
