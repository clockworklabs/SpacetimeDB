import { describe, expect, expectTypeOf, test } from 'vitest';
import { Timestamp } from '../src';
import type { Random } from '../src/server';
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

  test('shuffle is part of the public Random type', () => {
    expectTypeOf<Random['shuffle']>().toBeFunction();
  });

  test('shuffle handles empty and singleton arrays without drawing', () => {
    const random = makeRandom(new Timestamp(1n));
    random.integerInRange = () => {
      throw new Error('shuffle should not draw for fewer than two elements');
    };
    const empty: number[] = [];
    const singleton = ['only'];

    expect(random.shuffle(empty)).toBe(empty);
    expect(empty).toEqual([]);
    expect(random.shuffle(singleton)).toBe(singleton);
    expect(singleton).toEqual(['only']);
  });

  test('shuffle mutates and returns the input array', () => {
    const random = makeRandom(new Timestamp(1n));
    random.integerInRange = () => 0;
    const values = ['a', 'b', 'c', 'd'];

    expect(random.shuffle(values)).toBe(values);
    expect(values).toEqual(['b', 'c', 'd', 'a']);
  });

  test('shuffle uses the shrinking Fisher-Yates range on every iteration', () => {
    const random = makeRandom(new Timestamp(1n));
    const bounds: Array<[number, number]> = [];
    random.integerInRange = (min, max) => {
      bounds.push([min, max]);
      return max;
    };
    const values = [0, 1, 2, 3, 4];

    random.shuffle(values);

    expect(bounds).toEqual([
      [0, 4],
      [0, 3],
      [0, 2],
      [0, 1],
    ]);
    expect(values).toEqual([0, 1, 2, 3, 4]);
  });

  test('shuffle is deterministic for the same seed', () => {
    const first = makeRandom(new Timestamp(123456789n));
    const second = makeRandom(new Timestamp(123456789n));
    const values = Array.from({ length: 32 }, (_, index) => index);

    expect(first.shuffle([...values])).toEqual(second.shuffle([...values]));
  });

  test('shuffle uses the seed and preserves every element', () => {
    const values = Array.from({ length: 32 }, (_, index) => ({ index }));
    const first = makeRandom(new Timestamp(1n)).shuffle([...values]);
    const second = makeRandom(new Timestamp(2n)).shuffle([...values]);

    expect(first).not.toEqual(second);
    expect(new Set(first)).toEqual(new Set(values));
    expect(first).toHaveLength(values.length);
  });
});

function isNonZero(value: number | bigint): boolean {
  return typeof value === 'bigint' ? value !== 0n : value !== 0;
}
