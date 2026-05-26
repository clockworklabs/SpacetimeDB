import { describe, expect, test } from 'vitest';
import { Timestamp } from '../src';
import { makeRandom } from '../src/server/rng';

describe('Random', () => {
  test('fill is available and returns the same typed array', () => {
    const random = makeRandom(new Timestamp(1n));
    const bytes = new Uint8Array(16);

    expect(random.fill).toBeTypeOf('function');
    expect(random.fill(bytes)).toBe(bytes);
    expect(bytes.some(byte => byte !== 0)).toBe(true);
  });

  test('fill supports all integer typed arrays', () => {
    const random = makeRandom(new Timestamp(1n));
    const arrays = [
      new Int8Array(8),
      new Uint8Array(8),
      new Uint8ClampedArray(8),
      new Int16Array(8),
      new Uint16Array(8),
      new Int32Array(8),
      new Uint32Array(8),
      new BigInt64Array(8),
      new BigUint64Array(8),
    ] as const;

    for (const array of arrays) {
      random.fill(array);
      expect(Array.from(array).some(isNonZero)).toBe(true);
    }
  });

  test('fill handles empty typed arrays', () => {
    const random = makeRandom(new Timestamp(1n));
    const bytes = new Uint8Array();

    expect(random.fill(bytes)).toBe(bytes);
    expect(bytes).toHaveLength(0);
  });
});

function isNonZero(value: number | bigint): boolean {
  return typeof value === 'bigint' ? value !== 0n : value !== 0;
}
