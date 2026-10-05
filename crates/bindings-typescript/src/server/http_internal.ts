import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import status from 'statuses';
import { HttpRequest, HttpResponse } from '../lib/autogen/types';
import type { TimeDuration } from '../lib/time_duration';
import { bsatnBaseSize } from '../lib/util';
import {
  type BodyInit,
  type HeadersInit,
  deserializeHeaders,
  Headers,
  makeResponse,
  serializeHeaders,
  serializeMethod,
  SyncResponse,
} from './http_shared';
import { sys } from './runtime';

export { Headers };

const { freeze } = Object;

export interface RequestOptions {
  /** A BodyInit object or null to set request's body. */
  body?: BodyInit | null;
  /** A Headers object, an object literal, or an array of two-item arrays to set request's headers. */
  headers?: HeadersInit;
  /** A string to set request's method. */
  method?: string;
  /** A duration, after which the request will timeout */
  timeout?: TimeDuration;
  // /** A string indicating whether request follows redirects, results in an error upon encountering a redirect, or returns the redirect (in an opaque fashion). Sets request's redirect. */
  // redirect?: RequestRedirect;
}

export interface HttpClient {
  fetch(url: URL | string, init?: RequestOptions): SyncResponse;
}

const requestBaseSize = bsatnBaseSize({ types: [] }, HttpRequest.algebraicType);

function fetch(url: URL | string, init: RequestOptions = {}) {
  const method = serializeMethod(init.method);
  const headers = serializeHeaders(new Headers(init.headers as any));
  const uri = '' + url;
  const request: HttpRequest = freeze({
    method,
    headers,
    timeout: init.timeout,
    uri,
    version: { tag: 'Http11' } as const,
  });
  const requestBuf = new BinaryWriter(requestBaseSize);
  HttpRequest.serialize(requestBuf, request);
  const body =
    init.body == null
      ? new Uint8Array()
      : typeof init.body === 'string'
        ? init.body
        : new Uint8Array<ArrayBuffer>(init.body as any);
  const [responseBuf, responseBody] = sys.procedure_http_request(
    requestBuf.getBuffer(),
    body
  );
  const response = HttpResponse.deserialize(new BinaryReader(responseBuf));
  return SyncResponse[makeResponse](responseBody, {
    type: 'basic',
    url: uri,
    status: response.code,
    statusText: status(response.code),
    headers: deserializeHeaders(response.headers),
    aborted: false,
    version: response.version,
  });
}

freeze(fetch);

export const httpClient: HttpClient = freeze({ fetch });
