import assert from 'node:assert/strict';
import test from 'node:test';
import {
  redactCredentials,
  sanitiseConsoleError,
  sanitiseDiagnostic,
} from '../src/evidence/diagnostic-sanitizer.js';

const forbidden = /data-(?:role|testid)|getBy(?:TestId|Role|Text)|waitForSelector|locator\(|selectOption|control,#|data-state|localhost|127\.0\.0\.1|host\.docker\.internal|https?:\/\/|[A-Za-z]:[\\/]|\/tools\/stack-bench|\b\d+(?:ms|s)\b|runExpect/i;

test('sanitiser removes selector, timing, endpoint and harness-path mechanics', () => {
  const input = `Timeout 5000ms exceeded at C:\\repo\\tools\\stack-bench\\dist\\grader\\grade.js\n`
    + `waiting for locator('[data-testid="admin-panel"]') at http://127.0.0.1:6573/private\n`
    + `Call log:\n  - waiting for getByTestId('admin-panel')\n  - element is not enabled after 250ms\n`
    + `  - /tools/stack-bench/dist/grader/grade.js:99\n`
    + `  at runExpect (/tools/stack-bench/dist/grader/grade.js:99:2)\n`
    + `  waiting for getByRole('button', { name: 'secret grader label' }) for 2s`;
  const result = sanitiseDiagnostic(input);
  assert.doesNotMatch(result, forbidden);
  assert.match(result, /element is not enabled/i);
});

test('sanitiser handles single-quoted and unquoted test selectors', () => {
  const result = sanitiseDiagnostic("[data-role='one'],#one [data-test=two] [data-cy=three]");
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
    + '{"apiKey":"json-value","oauth_token":"oauth-value"} '
    + 'mongodb://user:database-password@database:27017/app';
  const result = redactCredentials(input);
  assert.doesNotMatch(result, /provider-value|subscription-value|json-value|oauth-value|database-password/);
  assert.equal(result.match(/\[redacted credential\]/g)?.length, 5);
});

test('Docker errors redact password arguments while retaining command and failure details', () => {
  const input = 'Command failed: docker exec backend mongosh --password "space secret" --eval ping\n'
    + "mongosh --password='quoted secret' --username appuser Authentication failed\n"
    + 'mongosh --password=plain-secret --quiet\n'
    + 'docker create --publish 127.0.0.1:5432:5432 -e PGPASSWORD=env-secret';
  const output = redactCredentials(input);
  assert.doesNotMatch(output, /space secret|quoted secret|plain-secret|env-secret/);
  assert.match(output, /Authentication failed/);
  assert.match(output, /--publish 127\.0\.0\.1:5432:5432/);
  assert.match(output, /--username appuser/);
});

test('SQL password redaction covers doubled quotes and escaped string literals', () => {
  const input = "CREATE USER appuser PASSWORD 'plain-secret'; ALTER ROLE appuser PASSWORD 'dou''bled-secret'; "
    + String.raw`ALTER ROLE appuser PASSWORD E'escaped\'secret'; syntax error near PASSWORD`;
  for (const output of [redactCredentials(input), sanitiseConsoleError(input)]) {
    assert.doesNotMatch(output, /plain-secret|dou|bled-secret|escaped|secret'/);
    assert.match(output, /CREATE USER appuser PASSWORD \[redacted credential\]/);
    assert.match(output, /syntax error near PASSWORD/);
  }
});
