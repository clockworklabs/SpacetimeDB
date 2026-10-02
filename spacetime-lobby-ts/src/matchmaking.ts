export const DEFAULT_RATING = 1000;
export const MIN_RATING = 100;
export const MAX_RATING = 5000;

const ELO_K = 32;
const RANKED_INITIAL_BAND = 100;
const RANKED_BAND_STEP = 50;
const RANKED_BAND_STEP_SECONDS = 10n;
const RANKED_MAX_BAND = 800;

type RankedTicket = {
  rating?: number | undefined;
  ratingPool?: string | undefined;
  createdAt: { microsSinceUnixEpoch: bigint };
};

export function rankedBand(ticket: RankedTicket, now: bigint): number {
  const waitedSeconds =
    ticket.createdAt.microsSinceUnixEpoch >= now
      ? 0n
      : (now - ticket.createdAt.microsSinceUnixEpoch) / 1_000_000n;
  const extra =
    Number(waitedSeconds / RANKED_BAND_STEP_SECONDS) * RANKED_BAND_STEP;
  return Math.min(RANKED_MAX_BAND, RANKED_INITIAL_BAND + extra);
}

/**
 * Picks `matchSize` tickets for the oldest anchor that can fill a match inside
 * its rating band, taking the closest ratings first. `queued` is oldest first.
 */
export function rankedSelection<T extends RankedTicket>(
  queued: T[],
  matchSize: number,
  now: bigint
): T[] | undefined {
  const ratingOf = (ticket: T) => ticket.rating ?? DEFAULT_RATING;
  const isOlder = (a: T, b: T) =>
    a.createdAt.microsSinceUnixEpoch < b.createdAt.microsSinceUnixEpoch;

  const pools = new Map<string | undefined, T[]>();
  for (const ticket of queued) {
    const pool = pools.get(ticket.ratingPool);
    if (pool) pool.push(ticket);
    else pools.set(ticket.ratingPool, [ticket]);
  }
  const position = new Map<T, number>();
  for (const pool of pools.values()) {
    pool.sort(
      (a, b) =>
        ratingOf(a) - ratingOf(b) ||
        (isOlder(a, b) ? -1 : isOlder(b, a) ? 1 : 0)
    );
    pool.forEach((ticket, index) => position.set(ticket, index));
  }

  for (const anchor of queued) {
    const pool = pools.get(anchor.ratingPool)!;
    if (pool.length < matchSize) continue;
    const anchorRating = ratingOf(anchor);
    const band = rankedBand(anchor, now);
    const picked = [anchor];
    let low = position.get(anchor)! - 1;
    let high = position.get(anchor)! + 1;
    while (picked.length < matchSize) {
      const left = low >= 0 ? pool[low] : undefined;
      const right = high < pool.length ? pool[high] : undefined;
      const leftGap = left ? anchorRating - ratingOf(left) : Infinity;
      const rightGap = right ? ratingOf(right) - anchorRating : Infinity;
      if (Math.min(leftGap, rightGap) > band) break;
      if (
        leftGap < rightGap ||
        (leftGap === rightGap && isOlder(left!, right!))
      ) {
        picked.push(left!);
        low--;
      } else {
        picked.push(right!);
        high++;
      }
    }
    if (picked.length === matchSize) return picked;
  }
  return undefined;
}

export function expectedScore(rating: number, opponentRating: number): number {
  return 1 / (1 + Math.pow(10, (opponentRating - rating) / 400));
}

export function updatedRating(
  rating: number,
  expected: number,
  score: number
): number {
  return Math.max(
    MIN_RATING,
    Math.min(MAX_RATING, Math.round(rating + ELO_K * (score - expected)))
  );
}
