import type { RandomGenerator } from 'pure-rand';
import { unsafeUniformBigIntDistribution } from 'pure-rand/distribution/UnsafeUniformBigIntDistribution';
import { unsafeUniformIntDistribution } from 'pure-rand/distribution/UnsafeUniformIntDistribution';
import { xoroshiro128plus } from 'pure-rand/generator/XoroShiro';
import type { Timestamp } from '../lib/timestamp';

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
 * A collection of random-number-generating functions.
 *
 * ## Usage
 *
 * ```
 * const floatOneToTen = ctx.random() * 10;
 * const randomBytes = ctx.random.fill(new Uint8Array(16));
 * const intOneToTen = ctx.random.integerInRange(0, 10);
 * const shuffledCards = ctx.random.shuffle(cards);
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
   * Shuffles an array in place using an unbiased Fisher-Yates shuffle, then
   * returns the same array.
   */
  shuffle<T>(array: T[]): T[];

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
const MUL = 6364136223846793005n;
const INC = 11634580027462260723n;

/** Based on the function of the same name in `rand_core::SeedableRng::seed_from_u64` */
function pcg32(state: bigint): number {
  state = asUintN(64, state * MUL + INC);
  const xorshifted = Number(asUintN(32, ((state >> 18n) ^ state) >> 27n));
  const rot = Number(asUintN(32, state >> 59n));
  // rotate `xorshifted` right by `rot` bits
  return ((xorshifted >>> rot) | (xorshifted << (32 - rot))) >>> 0;
}

/** From the `pure-rand` README */
function generateFloat64(rng: RandomGenerator): number {
  const g1 = unsafeUniformIntDistribution(0, (1 << 26) - 1, rng);
  const g2 = unsafeUniformIntDistribution(0, (1 << 27) - 1, rng);
  const value = (g1 * Math.pow(2, 27) + g2) * Math.pow(2, -53);
  return value;
}

export function makeRandomFromSeed(
  seed: Timestamp | number | bigint | Uint8Array
): Random {
  // Fill the 128-bit state with successive PCG32 outputs, as `seed_from_u64` does.
  let state = seedToBigInt(seed);
  const words: number[] = [];
  for (let i = 0; i < 4; i++) {
    words.push(pcg32(state) | 0);
    state = asUintN(64, state * MUL + INC);
  }
  const rng = xoroshiro128plus.fromState(words);

  const random: Random = () => generateFloat64(rng);

  random.fill = array => {
    if (isBigIntArray(array)) {
      const upper = (1n << BigInt(array.BYTES_PER_ELEMENT * 8)) - 1n;
      for (let i = 0; i < array.length; i++) {
        array[i] = unsafeUniformBigIntDistribution(0n, upper, rng);
      }
    } else {
      const upper = Math.pow(2, array.BYTES_PER_ELEMENT * 8) - 1;
      for (let i = 0; i < array.length; i++) {
        array[i] = unsafeUniformIntDistribution(0, upper, rng);
      }
    }
    return array;
  };

  random.shuffle = array => {
    for (
      let currentIndex = array.length - 1;
      currentIndex > 0;
      currentIndex--
    ) {
      const swapIndex = random.integerInRange(0, currentIndex);
      [array[currentIndex], array[swapIndex]] = [
        array[swapIndex],
        array[currentIndex],
      ];
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

export function makeRandom(seed: Timestamp): Random {
  return makeRandomFromSeed(seed);
}

function seedToBigInt(seed: Timestamp | number | bigint | Uint8Array): bigint {
  if (typeof seed === 'bigint') return seed;
  if (typeof seed === 'number') return BigInt(seed);
  if (seed instanceof Uint8Array) {
    let value = 0n;
    const len = Math.min(seed.byteLength, 8);
    for (let i = 0; i < len; i++) {
      value |= BigInt(seed[i]) << BigInt(i * 8);
    }
    return value;
  }
  return seed.microsSinceUnixEpoch;
}

function isBigIntArray(
  array: IntArray
): array is BigInt64Array | BigUint64Array {
  return array instanceof BigInt64Array || array instanceof BigUint64Array;
}
