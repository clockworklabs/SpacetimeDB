const MAX_AMOUNT_CENTS = (1n << 64n) - 1n;

export function formatMoney(cents: bigint) {
  const dollars = cents / 100n;
  const remainder = (cents % 100n).toString().padStart(2, '0');
  return `$${dollars.toLocaleString()}.${remainder}`;
}

export function parseMoney(input: string) {
  const value = input.trim();
  if (!/^\d+(?:\.\d{1,2})?$/.test(value)) return undefined;
  const [dollars, cents = ''] = value.split('.');
  const amount = BigInt(dollars) * 100n + BigInt(cents.padEnd(2, '0'));
  return amount > 0n && amount <= MAX_AMOUNT_CENTS ? amount : undefined;
}
