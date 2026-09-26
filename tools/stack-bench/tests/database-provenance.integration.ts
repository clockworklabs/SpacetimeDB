import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import test from 'node:test';
import { chromium, type Browser } from 'playwright';
import { loadTrack } from '../src/composition/tracks.js';
import { verifyApplicationDatabaseMarker, writeApplicationDatabaseMarker } from '../commands/run-suite.js';

test('browser signup provenance requires independent storage and releases its browser context', async () => {
  const stored = new Set<string>();
  let persist = false;
  let signedUp: string | null = null;
  const app = createServer(async (request, response) => {
    if (request.url === '/signup') {
      let body = '';
      for await (const chunk of request) body += String(chunk);
      const { username } = JSON.parse(body) as { username: string };
      signedUp = username;
      if (persist) stored.add(username);
      response.writeHead(200, { 'Content-Type': 'application/json' });
      response.end('{}');
      return;
    }
    response.writeHead(200, { 'Content-Type': 'text/html' });
    response.end(`<input id="signup-username"><input id="signup-password" type="password">
      <button id="signup-submit">Create account</button><script>
      document.querySelector('#signup-submit').onclick = async () => {
        const username = document.querySelector('#signup-username').value;
        await fetch('/signup', {method:'POST', body:JSON.stringify({username})});
        const current = document.createElement('strong'); current.id = 'current-user';
        current.textContent = username; document.body.append(current);
      };
      </script>`);
  });
  app.listen(0, '127.0.0.1');
  await once(app, 'listening');
  const address = app.address();
  assert(address && typeof address !== 'string');
  let browser: Browser | undefined;
  try {
    browser = await chromium.launch({ headless: true });
    const track = loadTrack('ecommerce');
    assert.deepEqual(track.databaseProvenance, { browserAction: 'signUp' });
    for (persist of [false, true]) {
      signedUp = null;
      let observed: string | null = null;
      const result = await verifyApplicationDatabaseMarker(
        { backend: 'postgres', url: `http://127.0.0.1:${address.port}` }, track.databaseProvenance, {
          write: (args, definition) => writeApplicationDatabaseMarker(args, definition, { browser }),
          read: (_args, marker) => {
            observed = marker ?? null;
            return { ok: !!marker && stored.has(marker), verified: true, reason: 'independent store' };
          },
        });
      assert.equal(result.write.ok, true, 'the same visible signup succeeds in both controls');
      assert(signedUp);
      assert.equal(observed, signedUp, 'the independent read looks for the marker the browser submitted');
      assert.equal(result.runtime?.ok, persist, 'a success page alone cannot prove persistence');
      assert.equal(browser.contexts().length, 0);
    }
  } finally {
    await browser?.close();
    await new Promise<void>((resolve, reject) => app.close(error => error ? reject(error) : resolve()));
  }
});
