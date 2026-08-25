const MAX_LOG_BODY = 2048;

export function truncateForLog(body: string): string {
  return body.length <= MAX_LOG_BODY
    ? body
    : `${body.slice(0, MAX_LOG_BODY)}...`;
}

export function toStatusCode(status: number): number {
  if (!Number.isInteger(status) || status < 0 || status > 0xffff) return 0;
  return status;
}

export function isOkStatus(status: number): boolean {
  return status >= 200 && status < 300;
}
