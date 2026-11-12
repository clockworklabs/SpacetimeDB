import assert from 'node:assert/strict';
import test from 'node:test';
import { classifyConvexFunctionResponse, convexFunctionRequest } from '../src/stacks/backends/convex-protocol.js';

test('Convex business requests use native functions and only end-user bearer auth', () => {
  const request = convexFunctionRequest({ deploymentUrl: 'http://localhost:3210/', kind: 'mutation',
    path: 'shop/orders:buy', args: { item: 'native-id', quantity: 1 }, token: 'user-token' });
  assert.equal(request.url, 'http://localhost:3210/api/mutation');
  assert.equal(request.method, 'POST');
  assert.deepEqual(request.headers, { 'Content-Type': 'application/json', Authorization: 'Bearer user-token' });
  assert.deepEqual(JSON.parse(request.body), {
    path: 'shop/orders:buy', args: { item: 'native-id', quantity: 1 }, format: 'json',
  });
  const anonymous = convexFunctionRequest({ deploymentUrl: 'http://localhost:3210', kind: 'query',
    path: 'shop/orders', args: {} });
  assert.equal(anonymous.headers.Authorization, undefined);
  assert.equal(JSON.parse(anonymous.body).path, 'shop/orders');
});

test('Convex request serialization must not silently erase malformed test inputs', () => {
  for (const bad of [undefined, NaN, Infinity, () => 1, Symbol('x'), 1n]) {
    assert.throws(() => convexFunctionRequest({ deploymentUrl: 'http://localhost:3210', kind: 'mutation',
      path: 'shop:buy', args: { quantity: bad } }), TypeError);
  }
  assert.throws(() => convexFunctionRequest({ deploymentUrl: 'http://admin:secret@localhost:3210',
    kind: 'query', path: 'shop:list', args: {} }), TypeError);
});

test('HTTP success requires a valid native success envelope, including null returns', () => {
  assert.deepEqual(classifyConvexFunctionResponse(200, '{"status":"success","value":null}'),
    { kind: 'accepted', httpStatus: 200, value: null });
  for (const body of ['{}', '[]', 'null', 'not JSON', '{"status":"success"}',
    '{"status":"success","value":1,"errorData":"failure"}',
    '{"status":"success","value":1,"logLines":[1]}']) {
    assert.equal(classifyConvexFunctionResponse(200, body).kind, 'invalid-response', body);
  }
  assert.equal(classifyConvexFunctionResponse(560, '{"status":"success","value":1}').kind, 'invalid-response');
});

test('ConvexError payloads identify application errors; HTTP 200 does not accept them', () => {
  for (const httpStatus of [200, 560]) {
    for (const errorData of [null, false, 0, '', { code: 'NO_STOCK' }]) {
      assert.deepEqual(classifyConvexFunctionResponse(httpStatus,
        JSON.stringify({ status: 'error', errorMessage: 'refused', errorData })),
      { kind: 'application-error', httpStatus, message: 'refused', errorData });
    }
  }
});

test('Unhandled and missing-function errors are not deliberate application refusals', () => {
  for (const message of ['Uncaught Error: failed', 'Could not find public function for shop:missing']) {
    assert.deepEqual(classifyConvexFunctionResponse(200, JSON.stringify({ status: 'error', errorMessage: message })),
      { kind: 'function-error', httpStatus: 200, message });
  }
  const nativeError = '{"code":"BadRequest","message":"missing function"}';
  assert.deepEqual(classifyConvexFunctionResponse(400, nativeError),
    { kind: 'http-error', httpStatus: 400, text: nativeError });
  assert.equal(classifyConvexFunctionResponse(503, '{"status":"success","value":1}').kind, 'http-error');
  assert.equal(classifyConvexFunctionResponse(200, '{"status":"error","errorData":{}}').kind, 'invalid-response');
});


test('pinned native argument validation is distinct from missing exports and user exceptions', () => {
  const message = '[Request ID: c3c0e4b69f8972e5] Server Error\nArgumentValidationError: Object contains extra field `username` that is not in the validator.\n';
  assert.equal(classifyConvexFunctionResponse(200, JSON.stringify({ status: 'error', errorMessage: message })).kind, 'validation-error');
  for (const errorMessage of [
    '[Request ID: c3c0e4b69f8972e5] Server Error\nCould not find public function for api:missing.',
    '[Request ID: c3c0e4b69f8972e5] Server Error\nUncaught Error: ArgumentValidationError: made up',
    'ArgumentValidationError: unqualified bare text',
  ]) assert.equal(classifyConvexFunctionResponse(200, JSON.stringify({ status: 'error', errorMessage })).kind, 'function-error');
  assert.equal(classifyConvexFunctionResponse(400, JSON.stringify({ status: 'error', errorMessage: message })).kind, 'http-error');
});
