import type { ProcedureModuleCtx } from './schema';
import type { PostHogConfig } from './config';

const MAX_LOG_BODY_LENGTH = 2048;

export function truncateForLog(body: string): string {
  return body.length <= MAX_LOG_BODY_LENGTH
    ? body
    : `${body.slice(0, MAX_LOG_BODY_LENGTH)}...`;
}

export function toStatusCode(status: number): number {
  if (!Number.isInteger(status) || status < 0 || status > 0xffff) return 0;
  return status;
}

export function isOkStatus(status: number): boolean {
  return status >= 200 && status < 300;
}

export type PostHogHttpResult = {
  ok: boolean;
  statusCode: number;
  responseBody: string;
};

export function posthogFetch(
  ctx: ProcedureModuleCtx,
  cfg: PostHogConfig,
  path: string,
  body: unknown
): PostHogHttpResult {
  const response = ctx.http.fetch(`${cfg.host}${path}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  const statusCode = toStatusCode(response.status);
  const responseBody = truncateForLog(response.text());
  return {
    ok: isOkStatus(statusCode),
    statusCode,
    responseBody,
  };
}
