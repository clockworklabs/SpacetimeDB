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
import type { Infer } from '../sdk';
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

export interface HttpClient {
  fetch(url: URL | string, init?: RequestOptions): SyncResponse;
}

const requestBaseSize = bsatnBaseSize({ types: [] }, HttpRequest.algebraicType);

const methods = new Map<string, Infer<typeof HttpMethod>>([
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

function fetch(url: URL | string, init: RequestOptions = {}) {
  const method = methods.get(init.method?.toUpperCase() ?? 'GET') ?? {
    tag: 'Extension',
    value: init.method!,
  };
  const headers: Infer<typeof HttpHeaders> = {
    // anys because the typings are wonky - see comment in SyncResponse.constructor
    entries: headersToList(new Headers(init.headers as any) as any)
      .flatMap(([k, v]) => (Array.isArray(v) ? v.map(v => [k, v]) : [[k, v]]))
      .map(([name, value]) => ({ name, value: textEncoder.encode(value) })),
  };
  const uri = '' + url;
  const request: Infer<typeof HttpRequest> = freeze({
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
    headers: new Headers(),
    aborted: false,
  });
}

freeze(fetch);

export const httpClient: HttpClient = freeze({ fetch });
