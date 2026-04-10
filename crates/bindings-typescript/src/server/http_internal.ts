import { Headers, headersToList } from 'headers-polyfill';
import status from 'statuses';
import BinaryReader from '../lib/binary_reader';
import BinaryWriter from '../lib/binary_writer';
import {
  HttpHeaders,
  HttpMethod,
  HttpRequest,
  HttpResponse,
} from '../lib/http_types';
import type { TimeDuration } from '../lib/time_duration';
import { bsatnBaseSize } from '../lib/util';
import { sys } from './runtime';

export { Headers };

const { freeze } = Object;

export type BodyInit = ArrayBuffer | ArrayBufferView | string;
export type HeadersInit = [string, string][] | Record<string, string> | Headers;
export interface ResponseInit {
  headers?: HeadersInit;
  status?: number;
  statusText?: string;
}

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder('utf-8' /* { fatal: true } */);

function deserializeHeaders(headers: HttpHeaders): Headers {
  return new Headers(
    headers.entries.map(({ name, value }): [string, string] => [name, textDecoder.decode(value)])
  );
}

const makeResponse = Symbol('makeResponse');

// based on deno's type of the same name
interface InnerResponse {
  type: 'basic' | 'cors' | 'default' | 'error' | 'opaque' | 'opaqueredirect';
  url: string | null;
  status: number;
  statusText: string;
  headers: Headers;
  aborted: boolean;
}

export class SyncResponse {
  #body: string | ArrayBuffer | null;
  #inner: InnerResponse;

  constructor(body?: BodyInit | null, init?: ResponseInit) {
    if (body == null) {
      this.#body = null;
    } else if (typeof body === 'string') {
      this.#body = body;
    } else {
      // this call is fine, the typings are just weird
      this.#body = new Uint8Array<ArrayBuffer>(body as any).buffer;
    }

    // there's a type mismatch - headers-polyfill's typing doesn't expect its
    // own `Headers` type, even though the actual code handles it correctly.
    this.#inner = {
      headers: new Headers(init?.headers as any),
      status: init?.status ?? 200,
      statusText: init?.statusText ?? '',
      type: 'default',
      url: null,
      aborted: false,
    };
  }

  static [makeResponse](body: BodyInit | null, inner: InnerResponse) {
    const me = new SyncResponse(body);
    me.#inner = inner;
    return me;
  }

  get headers(): Headers {
    return this.#inner.headers;
  }
  get status(): number {
    return this.#inner.status;
  }
  get statusText() {
    return this.#inner.statusText;
  }
  get ok(): boolean {
    return 200 <= this.#inner.status && this.#inner.status <= 299;
  }
  get url(): string {
    return this.#inner.url ?? '';
  }
  get type() {
    return this.#inner.type;
  }

  arrayBuffer(): ArrayBuffer {
    return this.bytes().buffer;
  }

  bytes(): Uint8Array<ArrayBuffer> {
    if (this.#body == null) {
      return new Uint8Array();
    } else if (typeof this.#body === 'string') {
      return textEncoder.encode(this.#body);
    } else {
      return new Uint8Array(this.#body);
    }
  }

  json(): any {
    return JSON.parse(this.text());
  }

  text(): string {
    if (this.#body == null) {
      return '';
    } else if (typeof this.#body === 'string') {
      return this.#body;
    } else {
      return textDecoder.decode(this.#body);
    }
  }
}

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

/**
 * A streaming HTTP response that yields body chunks via iteration.
 *
 * **Important:** Each iteration blocks the module's V8 thread until the next
 * chunk arrives. Because there is one V8 thread per module instance, all other
 * reducers and procedures for this database are stalled while waiting. For
 * long-running streams (e.g. LLM token streaming), this means the database is
 * unresponsive for the duration. Prefer streaming for large finite downloads
 * or cases where you can read a few chunks and exit early.
 */
export interface StreamingResponse extends Disposable {
  /** HTTP status code. */
  readonly status: number;
  /** HTTP status text. */
  readonly statusText: string;
  /** Response headers. */
  readonly headers: Headers;
  /** Whether the status is in the 200-299 range. */
  readonly ok: boolean;
  /** Iterate over response body chunks. Each iteration blocks until the next chunk arrives. */
  [Symbol.iterator](): Iterator<Uint8Array>;
  /** Close the underlying stream handle, canceling the background reader. */
  [Symbol.dispose](): void;
}

export interface HttpClient {
  fetch(url: URL | string, init?: RequestOptions): SyncResponse;
  /**
   * Initiate a streaming HTTP request. The response body can be iterated
   * chunk by chunk.
   *
   * **Important:** Iterating over the response blocks the module's V8 thread
   * on each chunk, stalling all other operations on this database until
   * iteration finishes or the stream is disposed. See {@link StreamingResponse}
   * for details.
   */
  fetchStreaming(url: URL | string, init?: RequestOptions): StreamingResponse;
}

const requestBaseSize = bsatnBaseSize({ types: [] }, HttpRequest.algebraicType);

const methods = new Map<string, HttpMethod>([
  ['GET', { tag: 'Get' }],
  ['HEAD', { tag: 'Head' }],
  ['POST', { tag: 'Post' }],
  ['PUT', { tag: 'Put' }],
  ['DELETE', { tag: 'Delete' }],
  ['CONNECT', { tag: 'Connect' }],
  ['OPTIONS', { tag: 'Options' }],
  ['TRACE', { tag: 'Trace' }],
  ['PATCH', { tag: 'Patch' }],
]);

function buildRequest(url: URL | string, init: RequestOptions = {}): { request: HttpRequest; uri: string; body: Uint8Array | string } {
  const method: HttpMethod = methods.get(init.method?.toUpperCase() ?? 'GET') ?? {
    tag: 'Extension',
    value: init.method!,
  };
  const headers: HttpHeaders = {
    // anys because the typings are wonky - see comment in SyncResponse.constructor
    entries: headersToList(new Headers(init.headers as any) as any)
      .flatMap(([k, v]) => (Array.isArray(v) ? v.map(v => [k, v]) : [[k, v]]))
      .map(([name, value]) => ({ name, value: textEncoder.encode(value) })),
  };
  const uri = '' + url;
  const request: HttpRequest = freeze({
    method,
    headers,
    timeout: init.timeout,
    uri,
    version: { tag: 'Http11' } as const,
  });
  const body =
    init.body == null
      ? new Uint8Array()
      : typeof init.body === 'string'
        ? init.body
        : new Uint8Array<ArrayBuffer>(init.body as any);
  return { request, uri, body };
}

function serializeRequest(request: HttpRequest): Uint8Array {
  const requestBuf = new BinaryWriter(requestBaseSize);
  HttpRequest.serialize(requestBuf, request);
  return requestBuf.getBuffer();
}

function fetch(url: URL | string, init: RequestOptions = {}) {
  const { request, uri, body } = buildRequest(url, init);
  const [responseBuf, responseBody] = sys.procedure_http_request(
    serializeRequest(request),
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
  });
}

/** Manages the lifecycle of a streaming HTTP response handle. */
class StreamHandle implements Disposable {
  #id: number | -1;

  static #finalizationRegistry = new FinalizationRegistry<number>(
    sys.procedure_http_stream_close
  );

  constructor(id: number) {
    this.#id = id;
    StreamHandle.#finalizationRegistry.register(this, id, this);
  }

  /** Read the next chunk. Returns null when the stream is exhausted. */
  next(): Uint8Array | null {
    if (this.#id === -1) return null;
    const chunk = sys.procedure_http_stream_next(this.#id);
    if (chunk === null) {
      this.#detach();
    }
    return chunk;
  }

  #detach(): number {
    const id = this.#id;
    this.#id = -1;
    StreamHandle.#finalizationRegistry.unregister(this);
    return id;
  }

  [Symbol.dispose]() {
    if (this.#id >= 0) {
      const id = this.#detach();
      sys.procedure_http_stream_close(id);
    }
  }
}

function fetchStreaming(url: URL | string, init: RequestOptions = {}): StreamingResponse {
  const { request, body } = buildRequest(url, init);
  const [handle, responseBuf] = sys.procedure_http_stream_open(
    serializeRequest(request),
    body
  );
  const stream = new StreamHandle(handle);
  const response = HttpResponse.deserialize(new BinaryReader(responseBuf));
  const code = response.code;
  const responseHeaders = deserializeHeaders(response.headers);

  return {
    get status() { return code; },
    get statusText() { return status(code) as string; },
    headers: responseHeaders,
    get ok() { return 200 <= code && code <= 299; },
    *[Symbol.iterator]() {
      try {
        let chunk: Uint8Array | null;
        while ((chunk = stream.next()) !== null) {
          yield chunk;
        }
      } finally {
        stream[Symbol.dispose]();
      }
    },
    [Symbol.dispose]() {
      stream[Symbol.dispose]();
    },
  };
}

freeze(fetch);
freeze(fetchStreaming);

export const httpClient: HttpClient = freeze({ fetch, fetchStreaming });
