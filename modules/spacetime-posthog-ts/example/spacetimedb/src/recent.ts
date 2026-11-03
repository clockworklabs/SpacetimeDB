export function newestFirst<
  T extends { createdAt: { microsSinceUnixEpoch: bigint } },
>(rows: T[]): T[] {
  return rows.sort((a, b) => {
    const av = a.createdAt.microsSinceUnixEpoch as bigint;
    const bv = b.createdAt.microsSinceUnixEpoch as bigint;
    return av < bv ? 1 : av > bv ? -1 : 0;
  });
}
