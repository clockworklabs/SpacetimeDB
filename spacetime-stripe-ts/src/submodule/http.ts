const STRIPE_API_ORIGIN = 'https://api.stripe.com';
const ALLOWED_METHODS = new Set(['GET', 'POST', 'DELETE']);
const MAX_PATH_LENGTH = 2048;
const MAX_FORM_BODY_LENGTH = 64 * 1024;
const MAX_IDEMPOTENCY_KEY_LENGTH = 255;

function hasControlCharacter(value: string): boolean {
  for (let index = 0; index < value.length; index++) {
    const code = value.charCodeAt(index);
    if (code <= 0x1f || code === 0x7f) return true;
  }
  return false;
}

export type StripeHttpRequest = {
  method: string;
  url: string;
  headers: Record<string, string>;
  body: string | undefined;
};

function validatePath(path: string): string {
  const normalized = path.trim();
  if (!normalized.startsWith('/v1/')) {
    throw new Error('stripe.request_path_invalid');
  }
  if (
    normalized.startsWith('//') ||
    normalized.includes('\\') ||
    normalized.includes('#')
  ) {
    throw new Error('stripe.request_path_invalid');
  }
  if (normalized.length > MAX_PATH_LENGTH || hasControlCharacter(normalized)) {
    throw new Error('stripe.request_path_invalid');
  }
  return normalized;
}

export function buildStripeHttpRequest(args: {
  method: string;
  path: string;
  secretKey: string;
  stripeVersion: string | undefined;
  formBody: string | undefined;
  idempotencyKey: string | undefined;
}): StripeHttpRequest {
  const method = args.method.trim().toUpperCase();
  if (!ALLOWED_METHODS.has(method)) {
    throw new Error('stripe.request_method_invalid');
  }

  const path = validatePath(args.path);
  const body = args.formBody?.length ? args.formBody : undefined;
  if (body !== undefined && body.length > MAX_FORM_BODY_LENGTH) {
    throw new Error('stripe.request_body_too_large');
  }
  if (
    args.idempotencyKey &&
    args.idempotencyKey.length > MAX_IDEMPOTENCY_KEY_LENGTH
  ) {
    throw new Error('stripe.idempotency_key_too_long');
  }

  const headers: Record<string, string> = {
    Authorization: `Bearer ${args.secretKey}`,
  };
  if (args.stripeVersion) headers['Stripe-Version'] = args.stripeVersion;
  if (args.idempotencyKey) headers['Idempotency-Key'] = args.idempotencyKey;
  if (body !== undefined)
    headers['Content-Type'] = 'application/x-www-form-urlencoded';

  return {
    method,
    url: `${STRIPE_API_ORIGIN}${path}`,
    headers,
    body,
  };
}
