import { describe, expect, test, vi } from 'vitest';
import { BinaryReader, BinaryWriter } from '../src';
import { HttpRequest, HttpResponse } from '../src/lib/autogen/types';
import { Headers, serializeHeaders } from '../src/server/http_shared';

const procedureHttpRequest = vi.hoisted(() => vi.fn());

vi.mock('../src/server/runtime', () => ({
  sys: { procedure_http_request: procedureHttpRequest },
}));

import { httpClient } from '../src/server/http_internal';

const textDecoder = new TextDecoder('utf-8');

function decoded(headers: HttpRequest['headers']): [string, string][] {
  return headers.entries.map(({ name, value }) => [
    name,
    textDecoder.decode(value),
  ]);
}

function serialized(headers: Headers): [string, string][] {
  return decoded(serializeHeaders(headers));
}

function respondWith(code: number) {
  const writer = new BinaryWriter(64);
  HttpResponse.serialize(writer, {
    headers: { entries: [] },
    version: { tag: 'Http11' },
    code,
  });
  procedureHttpRequest.mockReturnValueOnce([
    writer.getBuffer(),
    new Uint8Array(),
  ]);
}

describe('serializeHeaders', () => {
  test('keeps values that contain commas intact', () => {
    const headers = new Headers({
      'webhook-signature': 'v1,mf3NSumak46pFQ8AgeTg5fboQUCyeXw3TcawjLmuuSI=',
      'if-modified-since': 'Wed, 21 Oct 2015 07:28:00 GMT',
    });

    expect(serialized(headers)).toEqual([
      ['if-modified-since', 'Wed, 21 Oct 2015 07:28:00 GMT'],
      ['webhook-signature', 'v1,mf3NSumak46pFQ8AgeTg5fboQUCyeXw3TcawjLmuuSI='],
    ]);
  });

  test('keeps each set-cookie as its own entry', () => {
    const headers = new Headers();
    headers.append('set-cookie', 'a=1; Expires=Wed, 21 Oct 2015 07:28:00 GMT');
    headers.append('set-cookie', 'b=2');

    expect(serialized(headers)).toEqual([
      ['set-cookie', 'a=1; Expires=Wed, 21 Oct 2015 07:28:00 GMT'],
      ['set-cookie', 'b=2'],
    ]);
  });
});

describe('httpClient.fetch', () => {
  test('sends header values that contain commas intact', () => {
    respondWith(200);

    httpClient.fetch('https://example.com', {
      headers: { 'webhook-signature': 'v1,abc=' },
    });

    const [requestBuf] = procedureHttpRequest.mock.lastCall!;
    const request = HttpRequest.deserialize(new BinaryReader(requestBuf));
    expect(decoded(request.headers)).toEqual([
      ['webhook-signature', 'v1,abc='],
    ]);
  });

  test('returns responses with unknown status codes', () => {
    respondWith(530);

    const response = httpClient.fetch('https://example.com');

    expect(response.status).toBe(530);
    expect(response.statusText).toBe('');
  });

  test('sets the status text for known status codes', () => {
    respondWith(404);

    expect(httpClient.fetch('https://example.com').statusText).toBe(
      'Not Found'
    );
  });
});
