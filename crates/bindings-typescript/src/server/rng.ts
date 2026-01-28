import type { RandomGenerator } from 'pure-rand';
import { unsafeUniformBigIntDistribution } from 'pure-rand/distribution/UnsafeUniformBigIntDistribution';
import { unsafeUniformIntDistribution } from 'pure-rand/distribution/UnsafeUniformIntDistribution';
import { xoroshiro128plus } from 'pure-rand/generator/XoroShiro';
import type { Timestamp } from '../lib/timestamp';

declare global {
  interface Math {
    random(): never;
  }
}

type IntArray =
  | Int8Array
  | Uint8Array
  | Uint8ClampedArray
  | Int16Array
  | Uint16Array
  | Int32Array
  | Uint32Array
  | BigInt64Array
  | BigUint64Array;

/**
 * A collection of random-number-generating functions, seeded based on `ctx.timestamp`.
 *
 * ## Usage
 *
 * ```
 * const floatOneToTen = ctx.random() * 10;
 * const randomBytes = ctx.random.fill(new Uint8Array(16));
 * const intOneToTen = ctx.random.integerInRange(0, 10);
 * ```
 */
export interface Random {
  /**
   * Returns a random floating-point number in the range `[0.0, 1.0)`.
   *
   * The returned float will have 53 bits of randomness.
   */
  (): number;

  /**
   * Like `crypto.getRandomValues()`. Fills a `TypedArray` with random integers
   * in a uniform distribution, mutating and returning it.
   */
  fill<T extends IntArray>(array: T): T;

  /**
   * Returns a random unsigned 32-bit integer in a uniform distribution in the
   * range `[0, 2**32)`.
   */
  uint32(): number;

  /**
   * Returns an integer in the range `[min, max]`.
   */
  integerInRange(min: number, max: number): number;

  /**
   * Returns a bigint in the range `[min, max]`.
   */
  bigintInRange(min: bigint, max: bigint): bigint;
}

const { asUintN } = BigInt;

/** Based on the function of the same name in `rand_core::SeedableRng::seed_from_u64` */
function pcg32(state: bigint): number {
  const MUL = 6364136223846793005n;
  const INC = 11634580027462260723n;

  state = asUintN(64, state * MUL + INC);
  const xorshifted = Number(asUintN(32, ((state >> 18n) ^ state) >> 27n));
  const rot = Number(asUintN(32, state >> 59n));
  // rotate `xorshifted` right by `rot` bits
  return (xorshifted >> rot) | (xorshifted << (32 - rot));
}

/** From the `pure-rand` README */
function generateFloat64(rng: RandomGenerator): number {
  const g1 = unsafeUniformIntDistribution(0, (1 << 26) - 1, rng);
  const g2 = unsafeUniformIntDistribution(0, (1 << 27) - 1, rng);
  const value = (g1 * Math.pow(2, 27) + g2) * Math.pow(2, -53);
  return value;
}

export function makeRandom(seed: Timestamp): Random {
  // Use PCG32 to turn a 64-bit seed into a 32-bit seed, as the Rust `rand` crate does.
  const rng = xoroshiro128plus(pcg32(seed.microsSinceUnixEpoch));

  const random: Random = () => generateFloat64(rng);

  random.fill = array => {
    const elem = array.at(0);
    if (typeof elem === 'bigint') {
      const upper = (1n << BigInt(array.BYTES_PER_ELEMENT * 8)) - 1n;
      for (let i = 0; i < array.length; i++) {
        array[i] = unsafeUniformBigIntDistribution(0n, upper, rng);
      }
    } else if (typeof elem === 'number') {
      const upper = (1 << (array.BYTES_PER_ELEMENT * 8)) - 1;
      for (let i = 0; i < array.length; i++) {
        array[i] = unsafeUniformIntDistribution(0, upper, rng);
      }
    }
    return array;
  };

  random.uint32 = () => rng.unsafeNext();

  random.integerInRange = (min, max) =>
    unsafeUniformIntDistribution(min, max, rng);

  random.bigintInRange = (min, max) =>
    unsafeUniformBigIntDistribution(min, max, rng);

  return random;
}
