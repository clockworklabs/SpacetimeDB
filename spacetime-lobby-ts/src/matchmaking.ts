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

export function rankedSelection<T extends RankedTicket>(
  queued: T[],
  matchSize: number,
  now: bigint
): T[] | undefined {
  for (const anchor of queued) {
    const anchorRating = anchor.rating ?? DEFAULT_RATING;
    const band = rankedBand(anchor, now);
    const candidates = queued
      .filter(
        ticket =>
          Math.abs((ticket.rating ?? DEFAULT_RATING) - anchorRating) <= band
      )
      .filter(ticket => ticket.ratingPool === anchor.ratingPool)
      .sort((a, b) => {
        const ar = Math.abs((a.rating ?? DEFAULT_RATING) - anchorRating);
        const br = Math.abs((b.rating ?? DEFAULT_RATING) - anchorRating);
        if (ar !== br) return ar - br;
        const av = a.createdAt.microsSinceUnixEpoch;
        const bv = b.createdAt.microsSinceUnixEpoch;
        return av < bv ? -1 : av > bv ? 1 : 0;
      });
    if (candidates.length >= matchSize) return candidates.slice(0, matchSize);
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
