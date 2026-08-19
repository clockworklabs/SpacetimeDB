import assert from 'node:assert/strict';
import test from 'node:test';
import { humaniseDiagnostic, redactCredentials, sanitiseConsoleError,
  sanitiseDiagnostic } from '../src/evidence/diagnostic-sanitizer.mjs';

const forbidden = /data-testid|getBy(?:TestId|Role|Text)|waitForSelector|locator\(|localhost|127\.0\.0\.1|host\.docker\.internal|[A-Za-z]:[\\/]|\/tools\/stack-bench|\b\d+(?:ms|s)\b|runExpect/i;

test('sanitiser removes selector, timing, endpoint and harness-path mechanics', () => {
  const input = `Timeout 5000ms exceeded at C:\\repo\\tools\\stack-bench\\grader.mjs\n`
    + `waiting for locator('[data-testid="admin-panel"]') at http://127.0.0.1:6573/private\n`
    + `Call log:\n  - waiting for getByTestId('admin-panel')\n  - element is not enabled after 250ms\n`
    + `  - /tools/stack-bench/grader/grade.mjs:99\n`
    + `  at runExpect (/tools/stack-bench/grader/grade.mjs:99:2)\n`
    + `  waiting for getByRole('button', { name: 'secret grader label' }) for 2s`;
  const result = sanitiseDiagnostic(input);
  assert.doesNotMatch(result, forbidden);
  assert.match(result, /element is not enabled/i);
});

test('sanitiser handles single-quoted and unquoted test selectors', () => {
  const result = sanitiseDiagnostic("[data-testid='one'] [data-test=two] [data-cy=three]");
  assert.equal(result, 'the control the control the control');
});

test('credentials are redacted from diagnostics and console output', () => {
  const result = sanitiseConsoleError('Authorization: Bearer secret.payload api_key=sk-abcdefghijklmnop `oops`');
  assert.doesNotMatch(result, /secret\.payload|sk-abcdefghijklmnop|`/);
  assert.match(result, /redacted/i);
});

test('credential redaction covers provider environment and JSON spellings', () => {
  const input = 'ANTHROPIC_API_KEY=provider-value '
    + 'CLAUDE_CODE_OAUTH_TOKEN=subscription-value '
    + '{"apiKey":"json-value","oauth_token":"oauth-value"}';
  const result = redactCredentials(input);
  assert.doesNotMatch(result, /provider-value|subscription-value|json-value|oauth-value/);
  assert.equal(result.match(/\[redacted credential\]/g)?.length, 4);
});

test('humanisation keeps useful behaviour while hiding the implementation', () => {
  assert.equal(humaniseDiagnostic('[data-testid="toast"] not visible within 5000ms'), 'it never appeared');
  assert.equal(humaniseDiagnostic('ACCEPTED a write with a tampered ownerId'),
    'the server accepted a request that claimed to be from a different user');
  assert.equal(humaniseDiagnostic('setup failed: waiting for [data-testid="current-user"]'),
    'signing in never completed, so nothing behind it could be reached');
});
