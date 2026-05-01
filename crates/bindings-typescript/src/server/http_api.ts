import { Headers } from 'headers-polyfill';
import status from 'statuses';
import type { HttpVersion } from '../lib/http_types';

const textEncoder = new TextEncoder();
const textDecoder = new TextDecoder('utf-8');

export { Headers };

export type BodyInit = ArrayBuffer | ArrayBufferView | string;
export type HeadersInit = [string, string][] | Record<string, string> | Headers;

export interface RequestInit {
  body?: BodyInit | null;
  headers?: HeadersInit;
  method?: string;
  version?: HttpVersion;
}

export interface ResponseInit {
  headers?: HeadersInit;
  status?: number;
  statusText?: string;
  version?: HttpVersion;
}

type RequestInner = {
  headers: Headers;
  method: string;
  uri: string;
  version: HttpVersion;
};

type ResponseInner = {
  headers: Headers;
  status: number;
  statusText: string;
  version: HttpVersion;
};

export const makeRequest = Symbol('makeRequest');
export const makeResponse = Symbol('makeResponse');

export function coerceBody(
  body?: BodyInit | null
): string | ArrayBuffer | null {
  if (body == null) {
    return null;
  }
  if (typeof body === 'string') {
    return body;
  }
  // TODO(http): This currently drops byteOffset/byteLength for ArrayBufferView
  // inputs and can widen a sliced view to its full backing buffer. Fix in
  // both http_api.ts and http_internal.ts together so inbound/outbound HTTP
  // body handling stays aligned.
  return new Uint8Array<ArrayBuffer>(body as any).buffer;
}

export function bodyToBytes(
  body: string | ArrayBuffer | null
): Uint8Array<ArrayBuffer> {
  if (body == null) {
    return new Uint8Array();
  }
  if (typeof body === 'string') {
    return textEncoder.encode(body);
  }
  return new Uint8Array(body);
}

export function bodyToText(body: string | ArrayBuffer | null): string {
  if (body == null) {
    return '';
  }
  if (typeof body === 'string') {
    return body;
  }
  return textDecoder.decode(body);
}

function defaultStatusText(code: number) {
  try {
    return status(code);
  } catch {
    return '';
  }
}

export class Request {
  #body: string | ArrayBuffer | null;
  #inner: RequestInner;

  constructor(url: URL | string, init: RequestInit = {}) {
    this.#body = coerceBody(init.body);
    this.#inner = {
      headers: new Headers(init.headers as any),
      method: init.method?.toUpperCase() ?? 'GET',
      uri: '' + url,
      version: init.version ?? { tag: 'Http11' },
    };
  }

  static [makeRequest](body: BodyInit | null, inner: RequestInner) {
    const me = new Request(inner.uri);
    me.#body = coerceBody(body);
    me.#inner = inner;
    return me;
  }

  get headers(): Headers {
    return this.#inner.headers;
  }

  get method(): string {
    return this.#inner.method;
  }

  get uri(): string {
    return this.#inner.uri;
  }

  get url(): string {
    return this.#inner.uri;
  }

  get version(): HttpVersion {
    return this.#inner.version;
  }

  arrayBuffer(): ArrayBuffer {
    return this.bytes().buffer;
  }

  bytes(): Uint8Array<ArrayBuffer> {
    return bodyToBytes(this.#body);
  }

  json(): any {
    return JSON.parse(this.text());
  }

  text(): string {
    return bodyToText(this.#body);
  }
}

export class Response {
  #body: string | ArrayBuffer | null;
  #inner: ResponseInner;

  constructor(body?: BodyInit | null, init?: ResponseInit) {
    this.#body = coerceBody(body);
    const statusCode = init?.status ?? 200;
    this.#inner = {
      headers: new Headers(init?.headers as any),
      status: statusCode,
      statusText: init?.statusText ?? defaultStatusText(statusCode),
      version: init?.version ?? { tag: 'Http11' },
    };
  }

  static [makeResponse](body: BodyInit | null, inner: ResponseInner) {
    const me = new Response(body);
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

  get version(): HttpVersion {
    return this.#inner.version;
  }

  arrayBuffer(): ArrayBuffer {
    return this.bytes().buffer;
  }

  bytes(): Uint8Array<ArrayBuffer> {
    return bodyToBytes(this.#body);
  }

  json(): any {
    return JSON.parse(this.text());
  }

  text(): string {
    return bodyToText(this.#body);
  }
}
