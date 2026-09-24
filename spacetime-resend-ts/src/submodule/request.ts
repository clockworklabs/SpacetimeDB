import { hasControlCharacter } from './text-validation';

const RESEND_API_BASE = 'https://api.resend.com';
const ALLOWED_METHODS = new Set(['GET', 'POST', 'PATCH', 'DELETE']);
const MAX_PATH_LENGTH = 2048;
const MAX_JSON_BODY_LENGTH = 256 * 1024;
const MAX_IDEMPOTENCY_KEY_LENGTH = 256;

export type ResendHttpRequest = {
  method: string;
  url: string;
  headers: Record<string, string>;
  body: string | undefined;
};

function validatePath(path: string): string {
  const normalized = path.trim();
  if (!normalized.startsWith('/') || normalized.startsWith('//')) {
    throw new Error('resend.request_path_invalid');
  }
  if (normalized.includes('\\') || normalized.includes('#')) {
    throw new Error('resend.request_path_invalid');
  }
  if (normalized.length > MAX_PATH_LENGTH || hasControlCharacter(normalized)) {
    throw new Error('resend.request_path_invalid');
  }
  return normalized;
}

export function buildResendHttpRequest(args: {
  method: string;
  path: string;
  apiKey: string;
  jsonBody: string | undefined;
  idempotencyKey: string | undefined;
}): ResendHttpRequest {
  const method = args.method.trim().toUpperCase();
  if (!ALLOWED_METHODS.has(method)) {
    throw new Error('resend.request_method_invalid');
  }

  const path = validatePath(args.path);
  const body = args.jsonBody?.length ? args.jsonBody : undefined;
  if (body !== undefined && body.length > MAX_JSON_BODY_LENGTH) {
    throw new Error('resend.request_body_too_large');
  }
  if (
    args.idempotencyKey &&
    args.idempotencyKey.length > MAX_IDEMPOTENCY_KEY_LENGTH
  ) {
    throw new Error('resend.idempotency_key_too_long');
  }

  const headers: Record<string, string> = {
    Authorization: `Bearer ${args.apiKey}`,
    Accept: 'application/json',
  };
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  if (args.idempotencyKey) headers['Idempotency-Key'] = args.idempotencyKey;

  return {
    method,
    url: `${RESEND_API_BASE}${path}`,
    headers,
    body,
  };
}
