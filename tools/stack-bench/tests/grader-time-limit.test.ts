import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature, parseGradeArgs } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { evidenceIsMeasured } from '../src/evidence/check-evidence.js';

test('a check past the grading time limit fails and the rest of the grade is kept', async () => {
  const server = createServer((_request, response) => response.setHeader('content-type', 'text/html')
    .end(`<span id="ready">ready</span>
    <script>setTimeout(() => { const late = document.createElement('span'); late.id = 'late';
      late.textContent = 'late'; document.body.append(late); }, 2000);</script>`));
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  const url = `http://127.0.0.1:${address.port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const definition = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1,
      name: 'time limit', features: [{ id: 1, name: 'slow page', actors: ['buyer'], setup: [],
        criteria: [
          { id: '1a', desc: 'a step in progress at the limit completes', points: 1,
            steps: [{ do: 'expect', actor: 'buyer', testid: 'late', within: 5000 }] },
          { id: '1b', desc: 'the next check is past the limit', points: 1,
            steps: [{ do: 'expect', actor: 'buyer', testid: 'ready', within: 5000 }] },
        ] }] });
    const result = await gradeFeature(browser, definition.features[0]!, {
      url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false,
    }, { runId: 'time-limit', roomName: name => name, url, actions: [], spacetime: null, nullControl: false,
      timeLimit: { limitMs: 60_000, deadlineAtMs: Date.now() + 1000 } });
    const [started, late] = result.criteria;
    assert.equal(started!.evidence.status, 'passed');
    assert.equal(late!.evidence.status, 'failed');
    assert.equal(late!.evidence.finding?.kind, 'grading-time-limit');
    assert(evidenceIsMeasured(late!.evidence), 'a time-limit failure is measured, not inconclusive');
    assert.equal(result.score, 1);
  } finally {
    await browser.close();
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('the grading time limit must be a positive integer', () => {
  const base = ['node', 'grade.js', '--url', 'http://127.0.0.1:1', '--spec', 'spec.json'];
  assert.equal(parseGradeArgs([...base, '--time-limit-ms', '120000']).timeLimitMs, 120_000);
  for (const bad of ['0', '-1', '1.5', 'soon']) {
    assert.throws(() => parseGradeArgs([...base, `--time-limit-ms=${bad}`]), /positive integer/);
  }
});
