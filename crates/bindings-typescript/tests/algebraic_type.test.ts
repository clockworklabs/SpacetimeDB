import { describe, expect, test } from 'vitest';
import { AlgebraicType } from '../src/lib/algebraic_type';
import BinaryReader from '../src/lib/binary_reader';
import BinaryWriter from '../src/lib/binary_writer';
import { t } from '../src/lib/type_builders';

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
      expect(mapKey).toEqual(value);
    }
  });

  test('intoMapKey handles complex types', () => {
    const productType = AlgebraicType.Product({
      elements: [{ name: 'a', algebraicType: AlgebraicType.I32 }],
    });
    const productValue = { a: 42 };

    const mapKey = AlgebraicType.intoMapKey(productType, productValue);
    // Fallback for complex types is base64 encoding of serialized value
    expect(typeof mapKey).toEqual('string');
    // 42 as i32 little-endian is 2A000000, which is KgAAAA== in base64
    expect(mapKey).toEqual('KgAAAA==');
  });

  test('intoMapKey fallback serializes array types', () => {
    const arrayType = AlgebraicType.Array(AlgebraicType.U16);
    const arrayValue = [1, 2, 3];

    const mapKey = AlgebraicType.intoMapKey(arrayType, arrayValue);
    expect(typeof mapKey).toEqual('string');
    // Serialized as: [len (u32), val1 (u16), val2 (u16), val3 (u16)]
    expect(mapKey).toEqual('AwAAAAEAAgADAA==');
  });

  test('a result round-trips when its ok and err types differ', () => {
    const ty = t.result(t.u32(), t.string()).algebraicType;
    // The expected bytes come from the Rust `sats` encoder.
    const cases: Array<[object, string]> = [
      [{ ok: 7 }, '0007000000'],
      [{ err: 'boom' }, '0104000000626f6f6d'],
    ];
    for (const [value, hex] of cases) {
      const writer = new BinaryWriter(0);
      AlgebraicType.makeSerializer(ty)(writer, value);
      const bytes = writer.getBuffer();
      expect(
        Array.from(bytes, b => b.toString(16).padStart(2, '0')).join('')
      ).toBe(hex);
      const reader = new BinaryReader(bytes);
      expect(AlgebraicType.makeDeserializer(ty)(reader)).toEqual(value);
    }
  });
});
