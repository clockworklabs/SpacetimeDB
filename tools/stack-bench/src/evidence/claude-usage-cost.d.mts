import type { PricingRates } from './pricing-authority.js';

export const CLAUDE_SONNET_RATES: PricingRates;
export function claudeRatesForModel(model: string): PricingRates;
export function priceClaudeUsage(usage: unknown, rates?: PricingRates): number;
