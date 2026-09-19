// Native Functions API, not a synthetic REST wrapper. JSON covers the current
// ecommerce argument types; extended Convex values need a separate wire decision.
// https://docs.convex.dev/http-api/
export function convexFunctionRequest({ deploymentUrl, kind, path, args, token }: {
  deploymentUrl: string;
  kind: 'query' | 'mutation' | 'action';
  path: string;
  args: Readonly<Record<string, unknown>>;
  token?: string | null;
}) {
  const base = new URL(deploymentUrl);
  if (!['http:', 'https:'].includes(base.protocol) || base.username || base.password
      || base.search || base.hash) throw new TypeError('Invalid Convex deployment URL');
  if (!['query', 'mutation', 'action'].includes(kind) || !path.trim()) {
    throw new TypeError('Convex function kind and path are required');
  }
  if (!args || typeof args !== 'object' || Array.isArray(args)) {
    throw new TypeError('Convex function arguments must be an object');
  }
  const headers: Record<string, string> = { 'Content-Type': 'application/json' };
  // Only an end-user bearer token is supported. Deployment admin auth is never
  // part of the business-operation request path.
  if (token) {
    if (/[\r\n]/.test(token)) throw new TypeError('Invalid Convex bearer token');
    headers.Authorization = `Bearer ${token}`;
  }
  const body = JSON.stringify({ path, args, format: 'json' }, (_key, value: unknown) => {
    if (value === undefined || typeof value === 'function' || typeof value === 'symbol'
        || (typeof value === 'number' && !Number.isFinite(value))) {
      throw new TypeError('Convex JSON arguments must not lose values during serialization');
    }
    return value;
  });
  return { url: `${base.href.replace(/\/$/, '')}/api/${kind}`, method: 'POST', headers, body };
}

export type ConvexFunctionResponse = { readonly httpStatus: number } & (
  | { readonly kind: 'accepted'; readonly value: unknown }
  | { readonly kind: 'application-error'; readonly message: string; readonly errorData: unknown }
  | { readonly kind: 'validation-error'; readonly message: string }
  | { readonly kind: 'function-error'; readonly message: string }
  | { readonly kind: 'http-error'; readonly text: string }
  | { readonly kind: 'invalid-response'; readonly message: string }
);

// Call only after the response body is fully read. Fetch/body-read failures stay
// transport-unknown in the caller: a lost response does not prove a refusal.
export function classifyConvexFunctionResponse(httpStatus: number, text: string): ConvexFunctionResponse {
  // Convex's JS client accepts 560 for UDF errors, as well as HTTP success:
  // https://github.com/get-convex/convex-js/blob/main/src/browser/http_client.ts
  if (httpStatus !== 200 && httpStatus !== 560) return { kind: 'http-error', httpStatus, text };
  const invalid = (message: string): ConvexFunctionResponse => ({ kind: 'invalid-response', httpStatus, message });
  let body: unknown;
  try { body = JSON.parse(text); }
  catch { return invalid('Convex response is not JSON'); }
  if (!body || typeof body !== 'object' || Array.isArray(body)) return invalid('Missing Convex response envelope');
  const envelope = body as Record<string, unknown>;
  if (envelope.logLines !== undefined && (!Array.isArray(envelope.logLines)
      || !envelope.logLines.every(line => typeof line === 'string'))) {
    return invalid('Invalid Convex logLines');
  }
  if (envelope.status === 'success' && httpStatus === 200 && Object.hasOwn(envelope, 'value')
      && !Object.hasOwn(envelope, 'errorMessage') && !Object.hasOwn(envelope, 'errorData')) {
    return { kind: 'accepted', httpStatus, value: envelope.value };
  }
  if (envelope.status === 'error' && typeof envelope.errorMessage === 'string'
      && !Object.hasOwn(envelope, 'value')) {
    // The pinned backend emits this prefix before executing a function. A thrown
    // user exception is prefixed 'Uncaught', not an argument-validation refusal.
    // https://github.com/get-convex/convex-backend/blob/main/crates/udf/src/validation.rs
    if (!Object.hasOwn(envelope, 'errorData')
      && /^\[Request ID: [a-f0-9]{16}\] Server Error\nArgumentValidationError: /.test(envelope.errorMessage)) {
      return { kind: 'validation-error', httpStatus, message: envelope.errorMessage };
    }
    // Presence, not truthiness: ConvexError(null/false/0) also has errorData.
    return Object.hasOwn(envelope, 'errorData')
      ? { kind: 'application-error', httpStatus, message: envelope.errorMessage, errorData: envelope.errorData }
      : { kind: 'function-error', httpStatus, message: envelope.errorMessage };
  }
  return invalid('Invalid Convex success/error envelope');
}
