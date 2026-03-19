/** Fast, deterministic PRNG (mulberry32). */
function mulberry32(seed: number) {
  let t = seed >>> 0;
  return {
    next(): number {
      t += 0x6d2b79f5;
      let r = Math.imul(t ^ (t >>> 15), 1 | t);
      r ^= r + Math.imul(r ^ (r >>> 7), 61 | r);
      return ((r ^ (r >>> 14)) >>> 0) / 4294967296;
    },
  };
}

export type Rng = { next(): number };

export function makeRng(seed = 0x12345678): Rng {
  return mulberry32(seed);
}

/**
 * Pareto sampler (continuous) returning a function that yields samples in [xm, ∞).
 * CDF: 1 - (xm/x)^alpha  for x >= xm
 */
export function pareto(xm: number, alpha: number, seed?: number) {
  if (!(xm > 0)) throw new Error('xm must be > 0');
  if (!(alpha > 0)) throw new Error('alpha must be > 0');
  const rng = makeRng(seed);
  return () => {
    let u = 1 - rng.next(); // (0,1]
    if (u <= 0) u = Number.MIN_VALUE;
    return xm / Math.pow(u, 1 / alpha);
  };
}

/**
 * Zipf sampler with binary search.
 *
 * For very large N (>100k), consider using rejection sampling or
 * approximation methods instead.
 */
export function zipfSampler(N: number, s: number, seed?: number) {
  if (s <= 0) {
    // uniform for s <= 0
    const rng = makeRng(seed);
    return () => Math.floor(rng.next() * N);
  }

  const rng = makeRng(seed);

  // For small N, use precomputed CDF with binary search
  if (N <= 100_000) {
    return zipfCDF(N, s, rng);
  }

  // For very large N, use rejection sampling (more memory efficient)
  return zipfRejection(N, s, rng);
}

/**
 * CDF-based Zipf sampler with binary search.
 * Good for N up to ~100k.
 */
function zipfCDF(N: number, s: number, rng: Rng) {
  // Precompute CDF using Kahan summation for better numerical stability
  const cdf = new Float64Array(N);
  let sum = 0;
  let c = 0; // Kahan compensation

  for (let i = 0; i < N; i++) {
    const p = 1 / Math.pow(i + 1, s);
    // Kahan summation
    const y = p - c;
    const t = sum + y;
    c = t - sum - y;
    sum = t;
  }

  // Normalize to CDF
  let cumulative = 0;
  let comp = 0;
  for (let i = 0; i < N; i++) {
    const p = 1 / Math.pow(i + 1, s) / sum;
    const y = p - comp;
    const t = cumulative + y;
    comp = t - cumulative - y;
    cumulative = t;
    cdf[i] = cumulative;
  }
  cdf[N - 1] = 1.0; // ensure last value is exactly 1

  // Return sampler using binary search
  return () => {
    const u = rng.next();

    // Binary search for the first index where cdf[i] >= u
    let left = 0;
    let right = N - 1;

    while (left < right) {
      const mid = (left + right) >>> 1;
      if (cdf[mid] < u) {
        left = mid + 1;
      } else {
        right = mid;
      }
    }

    return left;
  };
}

/**
 * Rejection sampling for Zipf distribution.
 * More memory efficient for very large N, but slightly slower per sample.
 *
 * Uses the rejection method with a power-law proposal distribution.
 */
function zipfRejection(N: number, s: number, rng: Rng) {
  // Zeta function approximation for normalization
  const zeta = (() => {
    if (s === 1) {
      return Math.log(N) + 0.5772156649; // Euler-Mascheroni constant
    }
    let sum = 0;
    // Compute exact sum for first 1000 terms, approximate the rest
    const exact = Math.min(N, 1000);
    for (let i = 1; i <= exact; i++) {
      sum += 1 / Math.pow(i, s);
    }
    if (N > exact) {
      // Integral approximation for tail
      const a = exact;
      const b = N;
      sum += (Math.pow(a, 1 - s) - Math.pow(b, 1 - s)) / (s - 1);
    }
    return sum;
  })();

  // Proposal distribution parameter (power law with exponent s)
  const alpha = s;
  const xmin = 1;

  return () => {
    while (true) {
      // Sample from power law proposal: x^(-alpha) for x in [1, N]
      const u = rng.next();
      let x: number;

      if (alpha === 1) {
        x = Math.exp(u * Math.log(N));
      } else {
        const term = Math.pow(N, 1 - alpha) * u + (1 - u);
        x = Math.pow(term, 1 / (1 - alpha));
      }

      const k = Math.floor(x);
      if (k < 1 || k > N) continue;

      // Acceptance probability
      const px = 1 / (Math.pow(k, s) * zeta);
      const proposal =
        (alpha / (Math.pow(xmin, -alpha) - Math.pow(N + 1, -alpha))) *
        Math.pow(k, -alpha - 1);
      const M = Math.pow(N, s - alpha); // envelope constant

      const acceptProb = px / (M * proposal);

      if (rng.next() <= acceptProb) {
        return k - 1; // return 0-indexed
      }
    }
  };
}

/** Pick two distinct integers from a sampler. */
export function pickTwoDistinct(
  pick: () => number,
  maxSpins = 32,
): [number, number] {
  const a = pick();
  let b = pick();
  let spins = 0;
  while (b === a && spins++ < maxSpins) b = pick();
  return [a, b];
}
