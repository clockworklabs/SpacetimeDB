import * as assert from 'node:assert/strict';
import { buildResendHttpRequest } from '../src/submodule/request.ts';
import { validateEmailInput } from '../src/submodule/email-input.ts';
import { parseResendEventType } from '../src/submodule/webhook-metadata.ts';

assert.equal(
  parseResendEventType('{"type":"email.delivered","data":{}}'),
  'email.delivered'
);
assert.equal(parseResendEventType('{"type":"","data":{}}'), undefined);
assert.equal(parseResendEventType('{"data":{}}'), undefined);
assert.equal(parseResendEventType('{bad json'), undefined);
assert.equal(parseResendEventType('[]'), undefined);

const request = buildResendHttpRequest({
  method: 'post',
  path: '/emails',
  apiKey: 're_test_secret',
  jsonBody: '{"to":["user@example.com"]}',
  idempotencyKey: 'email-user-1',
});
assert.equal(request.url, 'https://api.resend.com/emails');
assert.equal(request.method, 'POST');
assert.equal(request.headers.Authorization, 'Bearer re_test_secret');
assert.throws(
  () =>
    buildResendHttpRequest({
      method: 'GET',
      path: 'https://attacker.example/collect',
      apiKey: 're_test_secret',
      jsonBody: undefined,
      idempotencyKey: undefined,
    }),
  /resend\.request_path_invalid/
);
assert.throws(
  () =>
    buildResendHttpRequest({
      method: 'GET',
      path: '/emails\u007fblocked',
      apiKey: 're_test_secret',
      jsonBody: undefined,
      idempotencyKey: undefined,
    }),
  /resend\.request_path_invalid/
);
assert.throws(
  () =>
    buildResendHttpRequest({
      method: 'TRACE',
      path: '/emails',
      apiKey: 're_test_secret',
      jsonBody: undefined,
      idempotencyKey: undefined,
    }),
  /resend\.request_method_invalid/
);

assert.doesNotThrow(() =>
  validateEmailInput({
    to: ['user@example.com'],
    subject: 'Welcome',
    text: 'Hello',
  })
);
assert.throws(
  () => validateEmailInput({ to: [], subject: 'Welcome', text: 'Hello' }),
  /resend\.send_email_no_recipients/
);
assert.throws(
  () =>
    validateEmailInput({
      to: ['user@example.com\u007fblocked'],
      subject: 'Welcome',
      text: 'Hello',
    }),
  /resend\.to_invalid_address/
);
assert.throws(
  () =>
    validateEmailInput({
      to: ['user@example.com'],
      subject: 'Welcome\r\nx-injected: yes',
      text: 'Hello',
    }),
  /resend\.send_email_invalid_subject/
);
assert.throws(
  () =>
    validateEmailInput({
      to: Array.from({ length: 101 }, (_, index) => `user${index}@example.com`),
      subject: 'Welcome',
      text: 'Hello',
    }),
  /resend\.to_too_many/
);

console.log('resend unit tests passed');
