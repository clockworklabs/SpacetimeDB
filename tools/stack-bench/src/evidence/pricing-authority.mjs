export const PRICING_UNIT = 'USD-per-million-tokens';
export const PRICING_RATE_FIELDS = Object.freeze([
  'input', 'output', 'cacheWrite5m', 'cacheWrite1h', 'cacheRead',
]);

const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);

export function validatePricingRates(value, { at = 'pricing rates' } = {}) {
  if (!object(value)) throw new Error(`${at} must be an object`);
  const keys = Object.keys(value);
  if (keys.length !== PRICING_RATE_FIELDS.length
    || keys.some(key => !PRICING_RATE_FIELDS.includes(key))) {
    throw new Error(`${at} must contain only ${PRICING_RATE_FIELDS.join(', ')}`);
  }
  for (const field of PRICING_RATE_FIELDS) {
    if (!Number.isFinite(value[field]) || value[field] < 0) {
      throw new Error(`${at}.${field} must be a non-negative number`);
    }
  }
  return Object.freeze(Object.fromEntries(
    PRICING_RATE_FIELDS.map(field => [field, value[field]])));
}

export function validatePricingAuthority(value, { at = 'pricing' } = {}) {
  if (!object(value) || Object.keys(value).some(key => !['unit', 'rates'].includes(key))) {
    throw new Error(`${at} must contain only unit and rates`);
  }
  if (value.unit !== PRICING_UNIT) {
    throw new Error(`${at}.unit must be ${PRICING_UNIT}`);
  }
  return Object.freeze({ unit: PRICING_UNIT,
    rates: validatePricingRates(value.rates, { at: `${at}.rates` }) });
}

export function pricingRatesEqual(left, right) {
  try {
    const a = validatePricingRates(left);
    const b = validatePricingRates(right);
    return PRICING_RATE_FIELDS.every(field => a[field] === b[field]);
  } catch {
    return false;
  }
}
