import type { ProcedureModuleCtx } from './schema';
import type { PostHogConfig } from './config';
import { isOkStatus, toStatusCode, truncateForLog } from './utils';

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
