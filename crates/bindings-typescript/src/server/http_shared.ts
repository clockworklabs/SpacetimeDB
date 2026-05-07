import { Headers, headersToList } from 'headers-polyfill';
import type {
  HttpHeaders,
  HttpMethod,
  HttpVersion,
} from '../lib/autogen/types';

export { Headers };

export type BodyInit = ArrayBuffer | ArrayBufferView | string;
export type HeadersInit = [string, string][] | Record<string, string> | Headers;

export const textEncoder = new TextEncoder();
export const textDecoder = new TextDecoder('utf-8');

export function deserializeMethod(method: HttpMethod): string {
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

export function serializeMethod(method?: string): HttpMethod {
  return (
    methods.get(method?.toUpperCase() ?? 'GET') ?? {
      tag: 'Extension',
      value: method!,
    }
  );
}

export function serializeHeaders(headers: Headers): HttpHeaders {
  return {
    entries: headersToList(headers as any)
      .flatMap(([k, v]) => (Array.isArray(v) ? v.map(v => [k, v]) : [[k, v]]))
      .map(([name, value]) => ({ name, value: textEncoder.encode(value) })),
  };
}

export function deserializeHeaders(headers: HttpHeaders): Headers {
  return new Headers(
    headers.entries.map(({ name, value }): [string, string] => [
      name,
      textDecoder.decode(value),
    ])
  );
}

export interface ResponseInit {
  headers?: HeadersInit;
  status?: number;
  statusText?: string;
  version?: HttpVersion;
}

export interface InnerResponse {
  type: 'basic' | 'cors' | 'default' | 'error' | 'opaque' | 'opaqueredirect';
  url: string | null;
  status: number;
  statusText: string;
  headers: Headers;
  aborted: boolean;
  version: HttpVersion;
}

export const makeResponse = Symbol('makeResponse');

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
      version: init?.version ?? { tag: 'Http11' },
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

  get version(): HttpVersion {
    return this.#inner.version;
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
