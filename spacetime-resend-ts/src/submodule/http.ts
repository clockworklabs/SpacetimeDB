import { type ProcedureModuleCtx, vResendErrorBody } from './schema';
import { attemptToParse, safeJsonParse, throwSenderError } from './utils';
import { buildResendHttpRequest } from './request';

export type ResendHttpResponse = {
  status: number;
  body: string;
};

export function callResend(
  ctx: ProcedureModuleCtx,
  args: {
    method: string;
    path: string;
    apiKey: string;
    jsonBody: string | undefined;
    idempotencyKey: string | undefined;
  }
): ResendHttpResponse {
  let request;
  try {
    request = buildResendHttpRequest(args);
  } catch (error) {
    throwSenderError(
      error instanceof Error ? error.message : 'resend.request_invalid'
    );
  }
  const response = ctx.http.fetch(request.url, {
    method: request.method,
    headers: request.headers,
    body: request.body,
  });
  return {
    status: response.status,
    body: response.text(),
  };
}
export function ensureOkOrThrow(
  response: ResendHttpResponse,
  errorPrefix: string
): void {
  if (response.status >= 200 && response.status < 300) return;
  throwSenderError(
    `${errorPrefix}:${response.status}${resendErrorSuffix(response.body)}`
  );
}

export function resendErrorSuffix(body: string): string {
  const parsed = safeJsonParse(body);
  if (parsed !== undefined) {
    const result = attemptToParse(vResendErrorBody, parsed);
    if (result.kind === 'success') {
      const parts: string[] = [];
      if (result.data.name) parts.push(`name=${result.data.name}`);
      if (result.data.message) {
        parts.push(
          `msg=${result.data.message.replace(/\s+/g, ' ').slice(0, 240)}`
        );
      }
      if (parts.length > 0) return `:${parts.join('|')}`;
    }
  }
  const compact = body.replace(/\s+/g, ' ').trim();
  if (!compact) return '';
  return `:body=${compact.slice(0, 240)}`;
}
