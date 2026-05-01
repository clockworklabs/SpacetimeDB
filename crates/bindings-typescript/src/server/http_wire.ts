import { headersToList } from 'headers-polyfill';
import type {
  HttpHeaders,
  HttpMethod,
  HttpRequest,
  HttpResponse,
} from '../lib/http_types';
import { type HttpClient, httpClient } from './http_internal';
import { Headers, Request, Response, makeRequest } from './http_api';

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder('utf-8');

function deserializeMethod(method: HttpMethod): string {
  switch (method.tag) {
    case 'Get':
      return 'GET';
    case 'Head':
      return 'HEAD';
    case 'Post':
      return 'POST';
    case 'Put':
      return 'PUT';
    case 'Delete':
      return 'DELETE';
    case 'Connect':
      return 'CONNECT';
    case 'Options':
      return 'OPTIONS';
    case 'Trace':
      return 'TRACE';
    case 'Patch':
      return 'PATCH';
    case 'Extension':
      return method.value;
  }
}

function deserializeHeaders(headers: HttpHeaders): Headers {
  return new Headers(
    headers.entries.map(({ name, value }): [string, string] => [
      name,
      textDecoder.decode(value),
    ])
  );
}

function serializeHeaders(headers: Headers): HttpHeaders {
  return {
    entries: headersToList(headers as any)
      .flatMap(([k, v]) => (Array.isArray(v) ? v.map(v => [k, v]) : [[k, v]]))
      .map(([name, value]) => ({ name, value: textEncoder.encode(value) })),
  };
}

export function requestFromWire(
  request: HttpRequest,
  body: Uint8Array
): Request {
  return Request[makeRequest](body, {
    headers: deserializeHeaders(request.headers),
    method: deserializeMethod(request.method),
    uri: request.uri,
    version: request.version,
  });
}

export function responseIntoWire(
  response: Response
): [HttpResponse, Uint8Array] {
  return [
    {
      headers: serializeHeaders(response.headers),
      version: response.version,
      code: response.status,
    },
    response.bytes(),
  ];
}

export function makeHandlerHttpClient(): HttpClient {
  return httpClient;
}
