import {
  validatePricingRates,
  type PricingRates,
} from './pricing-authority.mjs';

export const CLAUDE_SONNET_RATES: Readonly<PricingRates> = Object.freeze({
  input: 2.00,
  output: 10.00,
  cacheWrite5m: 2.50,
  cacheWrite1h: 4.00,
  cacheRead: 0.20,
});

export function claudeRatesForModel(model: unknown): Readonly<PricingRates> | null {
  return typeof model === 'string' && /^claude-sonnet-5(?:$|-)/.test(model)
    ? CLAUDE_SONNET_RATES : null;
}

export interface ClaudeUsage {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite5m: number;
  cacheWrite1h: number;
}

const nonNegativeNumber = (value: unknown, field: keyof ClaudeUsage): number => {
  if (typeof value !== 'number' || !Number.isFinite(value) || value < 0) {
    throw new Error(`Claude usage ${field} is incomplete`);
  }
  return value;
};

export function normalizeClaudeUsage(usage: unknown): ClaudeUsage {
  if (!usage || typeof usage !== 'object') throw new Error('complete Claude usage is required');
  const source = usage as Record<string, unknown>;
  const cacheCreationValue = source.cache_creation;
  const cacheCreation = cacheCreationValue && typeof cacheCreationValue === 'object'
    ? cacheCreationValue as Record<string, unknown>
    : {};
  const hasCacheBreakdown = Object.hasOwn(cacheCreation, 'ephemeral_5m_input_tokens')
    || Object.hasOwn(cacheCreation, 'ephemeral_1h_input_tokens');
  return {
    input: nonNegativeNumber(source.input_tokens, 'input'),
    output: nonNegativeNumber(source.output_tokens, 'output'),
    cacheRead: nonNegativeNumber(source.cache_read_input_tokens, 'cacheRead'),
    cacheWrite5m: hasCacheBreakdown
      ? nonNegativeNumber(cacheCreation.ephemeral_5m_input_tokens ?? 0, 'cacheWrite5m')
      : nonNegativeNumber(source.cache_creation_input_tokens, 'cacheWrite5m'),
    cacheWrite1h: nonNegativeNumber(
      cacheCreation.ephemeral_1h_input_tokens ?? 0,
      'cacheWrite1h',
    ),
  };
}

export function priceClaudeUsage(
  usage: unknown,
  rates: unknown = CLAUDE_SONNET_RATES,
): number {
  const validatedRates = validatePricingRates(rates, { at: 'Claude pricing rates' });
  const values = normalizeClaudeUsage(usage);
  return values.input * validatedRates.input / 1e6
    + values.output * validatedRates.output / 1e6
    + values.cacheRead * validatedRates.cacheRead / 1e6
    + values.cacheWrite5m * validatedRates.cacheWrite5m / 1e6
    + values.cacheWrite1h * validatedRates.cacheWrite1h / 1e6;
}
