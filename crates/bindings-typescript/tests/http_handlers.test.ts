import { describe, expect, it, vi } from 'vitest';

const registerExport = Symbol('SpacetimeDB.registerExport');
const exportContext = Symbol('SpacetimeDB.exportContext');

vi.mock('../src/server/schema', () => ({
  exportContext,
  registerExport,
}));

vi.mock('../src/server/http_internal', () => ({
  httpClient: {},
}));

describe('http request/response api', async () => {
  const { Request, SyncResponse } = await import('../src/server/http_handlers');

  it('preserves the provided request method string', () => {
    const request = new Request('https://example.test/items', {
      method: 'MyMethod',
    });

    expect(request.method).toBe('MyMethod');
  });

  it('reads request text, json, and bytes', () => {
    const request = new Request('https://example.test/items', {
      method: 'POST',
      body: JSON.stringify({ ok: true }),
    });

    expect(request.text()).toBe('{"ok":true}');
    expect(request.json()).toEqual({ ok: true });
    expect(Array.from(request.bytes())).toEqual(
      Array.from(new TextEncoder().encode('{"ok":true}'))
    );
  });

  it('defaults response status text to empty string', () => {
    const response = new SyncResponse('created', { status: 201 });

    expect(response.status).toBe(201);
    expect(response.statusText).toBe('');
    expect(response.ok).toBe(true);
  });

  it('marks non-2xx responses as not ok', () => {
    const response = new SyncResponse('teapot', { status: 418 });

    expect(response.ok).toBe(false);
    expect(response.text()).toBe('teapot');
  });

  it('supports array buffer bodies', () => {
    const response = new SyncResponse(new TextEncoder().encode('bytes'));

    expect(response.text()).toBe('bytes');
    expect(Array.from(response.bytes())).toEqual(
      Array.from(new TextEncoder().encode('bytes'))
    );
  });
});

describe('http handler exports', async () => {
  const { SyncResponse } = await import('../src/server/http_handlers');
  const { makeHttpHandlerExport } = await import('../src/server/http_handlers');

  function makeCtx() {
    return {
      moduleDef: {
        httpHandlers: [] as Array<{ sourceName: string }>,
        explicitNames: { entries: [] as unknown[] },
      },
      httpHandlers: [] as Array<unknown>,
      httpHandlerExports: new Map<object, string>(),
      defineHttpHandler(name: string) {
        if (this.httpHandlers.some(() => false)) {
          throw new TypeError(name);
        }
      },
    };
  }

  it('rejects exporting the same handler object more than once', () => {
    const ctx = makeCtx();
    const handler = makeHttpHandlerExport(ctx as never, undefined, () => {
      return new SyncResponse('ok');
    });

    handler[registerExport](ctx as never, 'hello');

    expect(() => handler[registerExport](ctx as never, 'helloAgain')).toThrow(
      "HTTP handler 'helloAgain' was exported more than once"
    );
  });

  it('allows distinct handler export objects for distinct handlers', () => {
    const ctx = makeCtx();
    const first = makeHttpHandlerExport(ctx as never, undefined, () => {
      return new SyncResponse('first');
    });
    const second = makeHttpHandlerExport(ctx as never, undefined, () => {
      return new SyncResponse('second');
    });

    expect(() => {
      first[registerExport](ctx as never, 'first');
      second[registerExport](ctx as never, 'second');
    }).not.toThrow();
  });

  it('records the originating schema context on the export', () => {
    const ctx = makeCtx();
    const handler = makeHttpHandlerExport(ctx as never, undefined, () => {
      return new SyncResponse('ok');
    });

    expect((handler as Record<symbol, unknown>)[exportContext]).toBe(ctx);
  });
});

describe('http router', async () => {
  const { Router } = await import('../src/server/http_handlers');
  type HttpHandlerExport =
    import('../src/server/http_handlers').HttpHandlerExport<any>;

  function handler(): HttpHandlerExport {
    return {} as HttpHandlerExport;
  }

  it('accepts strict root and slash root routes as distinct', () => {
    expect(() =>
      new Router().get('', handler()).get('/', handler()).get('/foo', handler())
    ).not.toThrow();
  });

  it('rejects paths without a leading slash unless they are empty root', () => {
    expect(() => new Router().get('foo', handler())).toThrow(
      'Route paths must start with `/`: foo'
    );
  });

  it('rejects invalid path characters', () => {
    expect(() => new Router().get('/Hello', handler())).toThrow(
      'Route paths may contain only ASCII lowercase letters, digits and `-_~/`: /Hello'
    );
  });

  it('allows distinct methods on the same path', () => {
    expect(() =>
      new Router().get('/echo', handler()).post('/echo', handler())
    ).not.toThrow();
  });

  it('rejects duplicate same-method same-path routes', () => {
    expect(() =>
      new Router().get('/echo', handler()).get('/echo', handler())
    ).toThrow('Route conflict for `/echo`');
  });

  it('rejects any() routes that overlap a method-specific route', () => {
    expect(() =>
      new Router().get('/echo', handler()).any('/echo', handler())
    ).toThrow('Route conflict for `/echo`');
  });

  it('treats trailing slash variants as distinct non-root routes', () => {
    expect(() =>
      new Router().get('/foo', handler()).get('/foo/', handler())
    ).not.toThrow();
  });

  it('nests paths by joining prefixes and suffixes', () => {
    const nested = new Router()
      .nest('/api', new Router().get('/users', handler()).get('/', handler()))
      .intoRoutes();

    expect(nested).toHaveLength(2);
    expect(nested.map(route => route.path)).toEqual(['/api/users', '/api']);
  });

  it('rejects nesting when an existing route overlaps the nested prefix', () => {
    expect(() =>
      new Router().get('/api/users', handler()).nest('/api', new Router())
    ).toThrow(
      'Cannot nest router at `/api`; existing routes overlap with nested path'
    );
  });

  it('treats sibling prefixes as overlapping nested paths', () => {
    expect(() =>
      new Router().get('/foobar', handler()).nest('/foo', new Router())
    ).toThrow(
      'Cannot nest router at `/foo`; existing routes overlap with nested path'
    );
  });

  it('preserves Rust trailing-slash behavior for nested empty paths', () => {
    const nested = new Router().nest(
      '/prefix',
      new Router().get('', handler())
    );

    expect(nested.intoRoutes().map(route => route.path)).toEqual(['/prefix/']);
  });

  it('rejects merge() conflicts', () => {
    expect(() =>
      new Router()
        .get('/echo', handler())
        .merge(new Router().get('/echo', handler()))
    ).toThrow('Route conflict for `/echo`');
  });
});
