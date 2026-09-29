export type RetryResult = { ok: true } | { ok: false; error: string };

export const retryOk = (): RetryResult => ({ ok: true });
export const retryFailed = (error: string): RetryResult => ({
  ok: false,
  error,
});
