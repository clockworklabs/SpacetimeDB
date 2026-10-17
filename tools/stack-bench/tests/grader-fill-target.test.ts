import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

test('fill types into the editable control when a listed value reuses its ID', async () => {
  // The promotion contract lists saved rules with the same field IDs as the form.
  const server = createServer((_request, response) => response.setHeader('content-type', 'text/html')
    .end(`<ul><li data-role="promotion-item"><strong data-role="promotion-code">SAVE10</strong></li></ul>
    <form data-role="promotion-form"><input data-role="promotion-code"></form>`));
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address();
  assert(address && typeof address !== 'string');
  const url = `http://127.0.0.1:${address.port}`;
  const browser = await chromium.launch({ headless: true });
  try {
    const definition = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1,
      name: 'fill target', features: [{ id: 1, name: 'promotions', actors: ['staff'], setup: [],
        criteria: [{ id: '1a', desc: 'a new code is typed into the form', points: 1, steps: [
          { do: 'fill', actor: 'staff', testid: 'promotion-code', text: 'NEW10' },
          { do: 'expect', actor: 'staff', testid: 'promotion-code', in: { testid: 'promotion-form' },
            value: 'NEW10', within: 2000 },
          { do: 'expect', actor: 'staff', testid: 'promotion-item', contains: 'SAVE10', within: 2000 },
        ] }] }] });
    const result = await gradeFeature(browser, definition.features[0]!, {
      url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false,
    }, { runId: 'fill-target', roomName: name => name, url, actions: [], spacetime: null, nullControl: false });
    assert.equal(result.criteria[0]!.evidence.status, 'passed', result.criteria[0]!.evidence.summary ?? '');
  } finally {
    await browser.close();
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
