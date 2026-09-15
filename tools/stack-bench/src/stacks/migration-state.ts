import { isDeepStrictEqual } from 'node:util';
import { z } from 'zod';
import { canonicalizeDefinition } from '../composition/definition-plan.js';
import { checkoutStateSchema, type CheckoutState } from './checkout-state.js';

const identifier = z.string().min(1);
const profile = z.strictObject({ name: z.string(), address: z.string() });
const savedProfileSchema = profile.extend({ accountId: identifier });
const addressBookSchema = z.strictObject({
  accountId: identifier,
  entries: z.array(z.strictObject({
    id: identifier, name: z.string(), address: z.string(), isDefault: z.boolean(),
  })),
  legacyProfile: profile,
});

// Readers must establish complete, authenticated observations before calling
// these comparators. Parse errors are measurement errors, never app failures.
// This is selected checkout coverage, not a whole-database migration reader.
const rows = <T>(values: readonly T[]) => values.map(value => canonicalizeDefinition(value))
  .sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
const checkout = (state: CheckoutState) => ({
  ...state,
  cart: rows(state.cart), stock: rows(state.stock), reservations: rows(state.reservations),
  payments: rows(state.payments),
  orders: rows(state.orders.map(order => ({ ...order,
    lines: rows(order.lines.map(line => ({ ...line, allocations: rows(line.allocations) }))),
  }))),
});

function uniqueScopes(values: readonly CheckoutState[]): void {
  const keys = values.map(value => JSON.stringify([value.accountId, value.itemId]));
  if (new Set(keys).size !== keys.length) throw new Error('duplicate migration checkout observation scope');
}

export function migrationCheckoutDifferences(before: unknown, after: unknown): string[] {
  const initial = z.array(checkoutStateSchema).min(1).parse(before);
  const observed = z.array(checkoutStateSchema).parse(after);
  uniqueScopes(initial);
  uniqueScopes(observed);
  if (!initial.some(value => value.orders.length > 0 && value.payments.length > 0)) {
    throw new Error('migration baseline must contain measured orders and payments');
  }
  const differences: string[] = [];
  if (!isDeepStrictEqual(rows(initial.map(({ accountId, itemId }) => ({ accountId, itemId }))),
    rows(observed.map(({ accountId, itemId }) => ({ accountId, itemId }))))) {
    differences.push('selected account/item scopes changed');
  }
  for (const original of initial) {
    const current = observed.find(value => value.accountId === original.accountId && value.itemId === original.itemId);
    if (!current) continue;
    const expected = checkout(original);
    const actual = checkout(current);
    for (const field of Object.keys(expected) as Array<keyof typeof expected>) {
      if (!isDeepStrictEqual(expected[field], actual[field])) {
        differences.push(`${original.accountId}/${original.itemId}: ${field} changed`);
      }
    }
  }
  return differences;
}

export function addressImportDifferences(before: unknown, after: unknown): string[] {
  const profiles = z.array(savedProfileSchema).min(1).parse(before);
  const books = z.array(addressBookSchema).parse(after);
  if (new Set(profiles.map(value => value.accountId)).size !== profiles.length) {
    throw new Error('duplicate starting profile observation');
  }
  const differences: string[] = [];
  if (!isDeepStrictEqual(rows(profiles.map(value => value.accountId)), rows(books.map(value => value.accountId)))) {
    differences.push('address-book account scopes changed');
  }
  for (const original of profiles) {
    const matches = books.filter(value => value.accountId === original.accountId);
    if (matches.length !== 1) continue;
    const book = matches[0]!;
    const populated = original.name !== '' || original.address !== '';
    if (book.entries.length !== Number(populated)) {
      differences.push(`${original.accountId}: imported address count differs`);
    }
    if (populated && book.entries.length === 1) {
      const entry = book.entries[0]!;
      if (entry.name !== original.name || entry.address !== original.address || !entry.isDefault) {
        differences.push(`${original.accountId}: imported text or default differs`);
      }
    }
    if (!isDeepStrictEqual(book.legacyProfile, { name: original.name, address: original.address })) {
      differences.push(`${original.accountId}: legacy profile differs`);
    }
  }
  return differences;
}
