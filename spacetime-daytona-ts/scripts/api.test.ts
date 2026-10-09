import assert from 'node:assert/strict';
import { test } from 'node:test';
import { api, ProviderError, type Http } from '../src/api.js';

test('sandbox creation sets the lifetime and access restrictions', () => {
  const http: Http = {
    fetch: (url, options) => {
      assert.equal(url, 'https://app.daytona.io/api/sandbox');
      assert.equal(options?.method, 'POST');
      assert.deepEqual(JSON.parse(String(options?.body)), {
        name: 'test-sandbox',
        labels: { database: 'test' },
        snapshot: 'daytona-small',
        ttlMinutes: 10,
        public: false,
        networkBlockAll: true,
        autoStopInterval: 0,
        autoDeleteInterval: 0,
      });
      return {
        status: 201,
        json: () => ({ id: 'remote', state: 'creating', labels: {} }),
      };
    },
  };
  assert.equal(
    api(http, 'key').create(
      'test-sandbox',
      { database: 'test' },
      'daytona-small',
      10
    ).id,
    'remote'
  );
});

test('provider errors do not expose response text or credentials', () => {
  const http: Http = {
    fetch: () => ({
      status: 429,
      json: () => {
        throw new Error('secret');
      },
    }),
  };
  assert.throws(() => api(http, 'private-key').sandbox('id'), {
    message: 'daytona.http_429',
  });
});

test('transport failure is an unknown outcome and is not retried', () => {
  let requests = 0;
  const http: Http = {
    fetch: () => {
      requests++;
      throw new Error('secret URL');
    },
  };
  assert.throws(
    () =>
      api(http, 'key').submit(
        'https://proxy.app.daytona.io/toolbox/id',
        'session',
        'exit 0'
      ),
    (error: unknown) =>
      error instanceof ProviderError && error.message === 'daytona.transport'
  );
  assert.equal(requests, 1);
});

test('a completed zero exit code differs from a running command', () => {
  let body: unknown = { id: 'cmd', command: 'exit 0' };
  const http: Http = { fetch: () => ({ status: 200, json: () => body }) };
  const service = api(http, 'key');
  const url = 'https://proxy.app.daytona.io/toolbox/id';
  assert.equal(service.command(url, 'session', 'cmd').exitCode, undefined);
  body = { id: 'cmd', command: 'exit 0', exitCode: 0 };
  assert.equal(service.command(url, 'session', 'cmd').exitCode, 0);
  body = { id: 'cmd', command: 'exit 0', exitCode: '0' };
  assert.throws(() => service.command(url, 'session', 'cmd'), {
    message: 'daytona.invalid_response',
  });
});

test('toolbox URLs cannot redirect credentials to an unapproved origin', () => {
  let requests = 0;
  const http: Http = {
    fetch: () => {
      requests++;
      return { status: 200, json: () => ({}) };
    },
  };
  const service = api(http, 'key');
  assert.throws(
    () => service.session('https://example.com/toolbox/id', 's'),
    /daytona.untrusted_toolbox/
  );
  assert.throws(
    () => service.session('http://proxy.app.daytona.io/toolbox/id', 's'),
    /daytona.untrusted_toolbox/
  );
  assert.equal(requests, 0);
});

test('async submission sends the documented fields and accepts only a command ID', () => {
  const http: Http = {
    fetch: (url, options) => {
      assert.equal(
        url,
        'https://proxy.app.daytona.io/toolbox/id/process/session/a%2Fb/exec'
      );
      assert.equal(options?.headers?.Authorization, 'Bearer key');
      assert.deepEqual(JSON.parse(String(options?.body)), {
        command: "sh -c 'exit 0'",
        runAsync: true,
      });
      return { status: 200, json: () => ({ cmdId: 'cmd' }) };
    },
  };
  assert.equal(
    api(http, 'key').submit(
      'https://proxy.app.daytona.io/toolbox/id',
      'a/b',
      'exit 0'
    ),
    'cmd'
  );
});
