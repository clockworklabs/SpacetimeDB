import { describe, expect, test } from 'vitest';
import { deepEqual } from '../src/lib/util';

function expectEqual(left: unknown, right: unknown, equal: boolean): void {
  expect(deepEqual(left, right)).toBe(equal);
  expect(deepEqual(right, left)).toBe(equal);
}

describe('deepEqual', () => {
  test('preserves primitive and reference equality', () => {
    const object = { value: 1 };
    expectEqual(object, object, true);
    expectEqual(null, null, true);
    expectEqual(null, {}, false);
    expectEqual(undefined, null, false);
    expectEqual(1, '1', false);
    expectEqual(1n, 1n, true);
    expectEqual(0, -0, true);
    expectEqual(NaN, NaN, false);
  });

  test('compares large byte arrays by value', () => {
    const left = new Uint8Array(100_000).fill(7);
    const right = left.slice();
    expectEqual(left, right, true);
    right[right.length - 1] = 8;
    expectEqual(left, right, false);
    expectEqual(left, left.subarray(1), false);
    expectEqual(new Uint8Array(), new Uint8Array(), true);
  });

  test('compares views using their own offsets and lengths', () => {
    const buffer = new Uint8Array([9, 1, 2, 3, 1, 2, 3, 8]);
    expectEqual(buffer.subarray(1, 4), buffer.subarray(4, 7), true);
    expectEqual(buffer.subarray(1, 4), new Uint8Array([1, 2, 3]), true);
    expectEqual(buffer.subarray(1, 4), buffer.subarray(2, 5), false);
  });

  test('compares blobs nested inside rows and arrays', () => {
    const left = { id: 1n, items: [{ data: new Uint8Array([1, 2, 3]) }] };
    const right = { id: 1n, items: [{ data: new Uint8Array([1, 2, 3]) }] };
    expectEqual(left, right, true);
    right.items[0].data[2] = 4;
    expectEqual(left, right, false);
  });

  test('compares enumerable properties attached to byte arrays', () => {
    const left = Object.assign(new Uint8Array([1, 2]), { label: 'a' });
    const right = Object.assign(new Uint8Array([1, 2]), { label: 'a' });
    expectEqual(left, right, true);
    right.label = 'b';
    expectEqual(left, right, false);
    expectEqual(left, new Uint8Array([1, 2]), false);
    expectEqual(
      left,
      Object.assign(new Uint8Array([1, 2]), { other: 'a' }),
      false
    );
  });

  test('compares large ordinary arrays and nested elements', () => {
    const left = Array.from({ length: 10_000 }, (_, i) => ({ value: i }));
    const right = Array.from({ length: 10_000 }, (_, i) => ({ value: i }));
    expectEqual(left, right, true);
    right[right.length - 1].value = -1;
    expectEqual(left, right, false);
    expectEqual([1, 2], [1, 2, 3], false);
  });

  test('preserves sparse-array enumerable-property semantics', () => {
    expectEqual(new Array(2), new Array(4), true);
    expectEqual(new Array(1), [undefined], false);
    const sparse = new Array<number>(2);
    sparse[1] = 1;
    expectEqual(sparse, [undefined, 1], false);
    const left = [1];
    left.length = 100;
    expectEqual(left, [1], true);
    expectEqual(
      Object.assign([1], { label: 'a' }),
      Object.assign([1], { label: 'b' }),
      false
    );
  });

  test('preserves structural equality across object kinds', () => {
    expectEqual([1, 2], { 0: 1, 1: 2 }, true);
    expectEqual(new Uint8Array([1, 2]), [1, 2], true);
    expectEqual(new Uint8Array([1, 2]), new Uint16Array([1, 2]), true);
  });

  test('ignores property insertion order', () => {
    expectEqual(
      { first: 1, second: { value: 2 } },
      { second: { value: 2 }, first: 1 },
      true
    );
  });

  test('requires matching own enumerable keys, even for undefined values', () => {
    expectEqual({ first: undefined }, { second: undefined }, false);
    expectEqual({ first: undefined }, {}, false);
    const inherited = Object.create({ first: undefined }) as Record<
      string,
      unknown
    >;
    inherited.second = undefined;
    expectEqual({ first: undefined }, inherited, false);
    const hidden = Object.defineProperty({ second: undefined }, 'first', {
      value: undefined,
    });
    expectEqual({ first: undefined }, hidden, false);
  });

  test('supports null prototypes and shadowed property-check methods', () => {
    const left = Object.assign(Object.create(null) as Record<string, unknown>, {
      value: 1,
    });
    expectEqual(left, { value: 1 }, true);
    expectEqual(
      { hasOwnProperty: 1, propertyIsEnumerable: 2 },
      { propertyIsEnumerable: 2, hasOwnProperty: 1 },
      true
    );
  });
});
