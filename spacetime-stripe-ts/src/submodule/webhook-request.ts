import { MAX_WEBHOOK_BODY_LENGTH, MAX_WEBHOOK_HEADER_LENGTH } from './limits';

export type WebhookRequestRejection = {
  status: number;
  error: string;
};

export function validateWebhookRequestHeaders(
  method: string,
  contentLengthHeader: string | null,
  signatureHeader: string | undefined
): WebhookRequestRejection | undefined {
  if (method !== 'POST') return { status: 405, error: 'method not allowed' };
  const contentLength = Number(contentLengthHeader ?? '0');
  if (
    Number.isFinite(contentLength) &&
    contentLength > MAX_WEBHOOK_BODY_LENGTH
  ) {
    return { status: 413, error: 'payload too large' };
  }
  if ((signatureHeader?.length ?? 0) > MAX_WEBHOOK_HEADER_LENGTH) {
    return { status: 431, error: 'signature header too large' };
  }
  return undefined;
}

export function validateWebhookRequestBody(
  payloadJson: string
): WebhookRequestRejection | undefined {
  if (payloadJson.length === 0) return { status: 400, error: 'empty body' };
  if (new TextEncoder().encode(payloadJson).length > MAX_WEBHOOK_BODY_LENGTH) {
    return { status: 413, error: 'payload too large' };
  }
  return undefined;
}
