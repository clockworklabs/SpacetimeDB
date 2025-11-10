import { describe, expect, test } from 'vitest';
import { AlgebraicType } from '../src/lib/algebraic_type';

describe('AlgebraicType', () => {
  test('intoMapKey handles all primitive types', () => {
    const primitiveTypes: Array<[any['tag'], any]> = [
      ['Bool', true],
      ['I8', -8],
      ['U8', 8],
      ['I16', -16],
      ['U16', 16],
      ['I32', -32],
      ['U32', 32],
      ['I64', -64n],
      ['U64', 64n],
      ['I128', -128n],
      ['U128', 128n],
      ['I256', -256n],
      ['U256', 256n],
      ['F32', 32.32],
      ['F64', 64.64],
      ['String', 'hello'],
    ];

    for (const [tag, value] of primitiveTypes) {
      const algebraicType = { tag, value: undefined };
      const mapKey = AlgebraicType.intoMapKey(algebraicType, value);
      expect(mapKey).toBe(value);
    }
  });

  test('intoMapKey handles complex types', () => {
    const productType = AlgebraicType.Product({
      elements: [{ name: 'a', algebraicType: AlgebraicType.I32 }],
    });
    const productValue = { a: 42 };

    const mapKey = AlgebraicType.intoMapKey(productType, productValue);
    // Fallback for complex types is base64 encoding of serialized value
    expect(typeof mapKey).toBe('string');
    // 42 as i32 little-endian is 2A000000, which is KgAAAA== in base64
    expect(mapKey).toBe('KgAAAA==');
  });

  test('intoMapKey fallback serializes array types', () => {
    const arrayType = AlgebraicType.Array(AlgebraicType.U16);
    const arrayValue = [1, 2, 3];

    const mapKey = AlgebraicType.intoMapKey(arrayType, arrayValue);
    expect(typeof mapKey).toBe('string');
    // Serialized as: [len (u32), val1 (u16), val2 (u16), val3 (u16)]
    expect(mapKey).toBe('AwAAAAEAAgADAA==');
  });
});
