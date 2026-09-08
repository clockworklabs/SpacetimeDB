/** Only trusted broker responses establish a resumable provider rejection. */
export interface ProviderFailure {
  category: 'rate-limit' | 'quota' | 'authentication' | 'transport' | 'request' | 'broker-budget';
  status: number | null;
  code: string | null;
}

export function classifyProviderFailure(status: number, body: Buffer): ProviderFailure {
  let code: string | null = null;
  try {
    const parsed = JSON.parse(body.toString('utf8'));
    const value = parsed?.error?.code ?? parsed?.error?.type;
    if (typeof value === 'string' && /^[a-zA-Z0-9_.-]{1,100}$/.test(value)) code = value;
  } catch { /* Status still identifies a rejection; never retain an arbitrary error body. */ }
  const category = ['insufficient_quota', 'billing_hard_limit_reached', 'credit_balance_too_low'].includes(code ?? '')
    || status === 402 ? 'quota'
    : status === 401 ? 'authentication'
      : status === 429 || status === 529 ? 'rate-limit' : 'request';
  return { category, status, code };
}
