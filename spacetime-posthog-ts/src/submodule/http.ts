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

export function featureFlagValue(
  body: string,
  key: string
): boolean | string | undefined {
  try {
    const parsed: unknown = JSON.parse(body);
    if (!parsed || typeof parsed !== 'object' || !('flags' in parsed))
      return undefined;
    const flags = parsed.flags;
    if (
      !flags ||
      typeof flags !== 'object' ||
      !Object.prototype.hasOwnProperty.call(flags, key)
    )
      return undefined;
    const flag: unknown = (flags as Record<string, unknown>)[key];
    if (
      !flag ||
      typeof flag !== 'object' ||
      !('enabled' in flag) ||
      typeof flag.enabled !== 'boolean'
    )
      return undefined;
    return 'variant' in flag && typeof flag.variant === 'string'
      ? flag.variant
      : flag.enabled;
  } catch {
    return undefined;
  }
}

type FetchCtx = {
  http: {
    fetch(
      url: string,
      init: { method: string; headers: Record<string, string>; body: string }
    ): { status: number; text(): string };
  };
};

/** Network failures return status code 0 with the error message as the body. */
export function posthogFetch(
  ctx: FetchCtx,
  cfg: PostHogConfig,
  path: string,
  body: unknown
): PostHogHttpResult {
  let response;
  try {
    response = ctx.http.fetch(`${cfg.host}${path}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });
  } catch (error) {
    return {
      ok: false,
      statusCode: 0,
      responseBody: error instanceof Error ? error.message : String(error),
    };
  }
  const statusCode = toStatusCode(response.status);
  return {
    ok: isOkStatus(statusCode),
    statusCode,
    responseBody: response.text(),
  };
}
