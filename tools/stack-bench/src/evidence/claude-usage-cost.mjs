import { validatePricingRates } from './pricing-authority.mjs';

export const CLAUDE_SONNET_RATES = Object.freeze({
  input: 2.00,
  output: 10.00,
  cacheWrite5m: 2.50,
  cacheWrite1h: 4.00,
  cacheRead: 0.20,
});

export function claudeRatesForModel(model) {
  return typeof model === 'string' && /^claude-sonnet-5(?:$|-)/.test(model)
    ? CLAUDE_SONNET_RATES : null;
}

export function normalizeClaudeUsage(usage) {
  if (!usage || typeof usage !== 'object') throw new Error('complete Claude usage is required');
  const cacheCreation = usage.cache_creation ?? {};
  const hasCacheBreakdown = Object.hasOwn(cacheCreation, 'ephemeral_5m_input_tokens')
    || Object.hasOwn(cacheCreation, 'ephemeral_1h_input_tokens');
  const values = {
    input: usage.input_tokens,
    output: usage.output_tokens,
    cacheRead: usage.cache_read_input_tokens,
    cacheWrite5m: hasCacheBreakdown
      ? cacheCreation.ephemeral_5m_input_tokens ?? 0
      : usage.cache_creation_input_tokens,
    cacheWrite1h: cacheCreation.ephemeral_1h_input_tokens ?? 0,
  };
  for (const [field, value] of Object.entries(values)) {
    if (!Number.isFinite(value) || value < 0) {
      throw new Error(`Claude usage ${field} is incomplete`);
    }
  }
  return values;
}

export function priceClaudeUsage(usage, rates = CLAUDE_SONNET_RATES) {
  rates = validatePricingRates(rates, { at: 'Claude pricing rates' });
  const values = normalizeClaudeUsage(usage);
  return values.input * rates.input / 1e6
    + values.output * rates.output / 1e6
    + values.cacheRead * rates.cacheRead / 1e6
    + values.cacheWrite5m * rates.cacheWrite5m / 1e6
    + values.cacheWrite1h * rates.cacheWrite1h / 1e6;
}
