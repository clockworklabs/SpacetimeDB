import assert from 'node:assert/strict';
import test from 'node:test';

import { CLAUDE_SONNET_RATES, claudeRatesForModel, priceClaudeUsage }
  from '../src/evidence/claude-usage-cost.mjs';
import { PRICING_UNIT, validatePricingAuthority }
  from '../src/evidence/pricing-authority.mjs';

test('the direct Sonnet 5 fallback uses the recorded five-rate schedule', () => {
  assert.deepEqual(CLAUDE_SONNET_RATES, {
    input: 2, output: 10, cacheWrite5m: 2.5, cacheWrite1h: 4, cacheRead: 0.2,
  });
  assert.equal(claudeRatesForModel('claude-sonnet-5'), CLAUDE_SONNET_RATES);
  assert.equal(priceClaudeUsage({
    input_tokens: 1_000_000,
    output_tokens: 1_000_000,
    cache_read_input_tokens: 1_000_000,
    cache_creation: {
      ephemeral_5m_input_tokens: 1_000_000,
      ephemeral_1h_input_tokens: 1_000_000,
    },
  }), 18.7);
});

test('pricing authority requires one unit and the exact five-rate shape', () => {
  const value = { unit: PRICING_UNIT, rates: CLAUDE_SONNET_RATES };
  assert.deepEqual(validatePricingAuthority(value), value);
  assert.throws(() => validatePricingAuthority({ ...value, unit: 'USD-per-token' }),
    /pricing\.unit/);
  assert.throws(() => validatePricingAuthority({ ...value,
    rates: { ...value.rates, cacheWrite: 3 } }), /contain only/);
});
