import type { Timestamp } from 'spacetimedb';

export function latestSubscription<
  T extends { insertedAt: Timestamp; stripeSubscriptionId: string },
>(rows: Iterable<T>): T | undefined {
  let latest: T | undefined;
  for (const row of rows) {
    if (
      !latest ||
      row.insertedAt.microsSinceUnixEpoch >
        latest.insertedAt.microsSinceUnixEpoch ||
      (row.insertedAt.microsSinceUnixEpoch ===
        latest.insertedAt.microsSinceUnixEpoch &&
        row.stripeSubscriptionId > latest.stripeSubscriptionId)
    ) {
      latest = row;
    }
  }
  return latest;
}
