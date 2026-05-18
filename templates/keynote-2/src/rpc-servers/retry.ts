// Retry serialization (40001), deadlock (40P01), lock-not-available (55P03).
const RETRYABLE_SQLSTATES = new Set(['40001', '40P01', '55P03']);

export type RetryOptions = {
  maxAttempts?: number;
  baseDelayMs?: number;
  maxDelayMs?: number;
  onRetry?: (attempt: number, err: unknown) => void;
};

export class RetryExhaustedError extends Error {
  constructor(
    public readonly attempts: number,
    public readonly lastError: unknown,
  ) {
    const cause =
      (lastError as { message?: string })?.message ?? String(lastError);
    super(`retry exhausted after ${attempts} attempts: ${cause}`);
    this.name = 'RetryExhaustedError';
  }
}

function getSqlstate(err: unknown): string | undefined {
  let cursor: any = err;
  for (let i = 0; i < 5 && cursor; i++) {
    const code = cursor.code;
    if (typeof code === 'string' && /^[0-9A-Z]{5}$/.test(code)) return code;
    cursor = cursor.cause ?? cursor.originalError ?? cursor.innerError ?? null;
  }
  return undefined;
}

function isRetryable(err: unknown): boolean {
  const code = getSqlstate(err);
  return code !== undefined && RETRYABLE_SQLSTATES.has(code);
}

export async function withTxnRetry<T>(
  fn: () => Promise<T>,
  options: RetryOptions = {},
): Promise<T> {
  const maxAttempts = options.maxAttempts ?? 10;
  const baseDelayMs = options.baseDelayMs ?? 5;
  const maxDelayMs = options.maxDelayMs ?? 200;

  let lastError: unknown;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    try {
      return await fn();
    } catch (err) {
      lastError = err;
      if (!isRetryable(err)) throw err;
      if (attempt === maxAttempts) throw new RetryExhaustedError(attempt, err);
      options.onRetry?.(attempt, err);
      const cap = Math.min(maxDelayMs, baseDelayMs * 2 ** attempt);
      const delay = Math.floor(Math.random() * cap);
      if (delay > 0) await new Promise((r) => setTimeout(r, delay));
    }
  }
  throw new RetryExhaustedError(maxAttempts, lastError);
}
