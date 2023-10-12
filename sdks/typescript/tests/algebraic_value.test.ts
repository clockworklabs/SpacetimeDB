import {
  AlgebraicType,
  ProductType,
  ProductTypeElement,
} from "../src/algebraic_type";
import {
  ProductValue,
  AlgebraicValue,
  SumValue,
  BinaryAdapter,
  JSONAdapter,
} from "../src/algebraic_value";
import BinaryReader from "../src/binary_reader";

describe("AlgebraicValue", () => {
  test("when created with a ProductValue it assigns the product property", () => {
    let value = new ProductValue([]);
    let av = new AlgebraicValue(value);

    expect(av.asProductValue()).toBe(value);
  });

  test("when created with a SumValue it assigns the sum property", () => {
    let value = new SumValue(1, new AlgebraicValue(1));
    let av = new AlgebraicValue(value);

    expect(av.asSumValue()).toBe(value);
  });

  test("when created with a AlgebraicValue(string) it can be requested as a string", () => {
    let av = new AlgebraicValue("foo");

    expect(av.asString()).toBe("foo");
  });

  test("when created with a AlgebraicValue(AlgebraicValue[]) it can be requested as an array", () => {
    let array: AlgebraicValue[] = [new AlgebraicValue(1)];
    let av = new AlgebraicValue(array);

    expect(av.asArray()).toBe(array);
  });
});

describe("primitive values", () => {
  describe("deserialize with a binary adapter", () => {
    test("should correctly deserialize array with U8 type", () => {
      const input = new Uint8Array([2, 0, 0, 0, 10, 20]);
      const reader = new BinaryReader(input);
      const adapter: BinaryAdapter = new BinaryAdapter(reader);
      const type = AlgebraicType.createBytesType();
      const result = AlgebraicValue.deserialize(type, adapter);

      expect(result.asBytes()).toEqual(new Uint8Array([10, 20]));
    });

    test("should correctly deserialize array with U128 type", () => {
      // byte array of length 0002
      // prettier-ignore
      const input = new Uint8Array([
        3, 0, 0, 0, // 4 bytes for length
        1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 16 bytes for u128
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 16 bytes for max u128
        10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, // 16 bytes for u128
      ]);
      const reader = new BinaryReader(input);
      const adapter: BinaryAdapter = new BinaryAdapter(reader);
      const type = AlgebraicType.createArrayType(AlgebraicType.createU128Type());
      const result = AlgebraicValue.deserialize(type, adapter);

      const u128_max = BigInt(2) ** BigInt(128) - BigInt(1);
      expect(result.asJsArray("BigInt")).toEqual([
        BigInt(1),
        u128_max,
        BigInt(10),
      ]);
    });

    test("should correctly deserialize an U128 type", () => {
      // byte array of length 0002
      // prettier-ignore
      const input = new Uint8Array([
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, // 16 bytes for max u128
      ]);
      const reader = new BinaryReader(input);
      const adapter: BinaryAdapter = new BinaryAdapter(reader);
      const result = AlgebraicValue.deserialize(
        AlgebraicType.createU128Type(),
        adapter
      );

      const u128_max = BigInt(2) ** BigInt(128) - BigInt(1);
      expect(result.asBigInt()).toEqual(u128_max);
    });

    test("should correctly deserialize a boolean type", () => {
      // byte array of length 0002
      const input = new Uint8Array([1]);
      const reader = new BinaryReader(input);
      const adapter: BinaryAdapter = new BinaryAdapter(reader);
      const result = AlgebraicValue.deserialize(
        AlgebraicType.createBoolType(),
        adapter
      );

      expect(result.asBool()).toEqual(true);
    });

    test("should correctly deserialize a string type", () => {
      // byte array of length 0002
      const text = "zażółć gęślą jaźń";
      const encoder = new TextEncoder();
      const textBytes = encoder.encode(text);

      const input = new Uint8Array(textBytes.length + 4);
      input.set(new Uint8Array([textBytes.length, 0, 0, 0]));
      input.set(textBytes, 4);

      const reader = new BinaryReader(input);
      const adapter: BinaryAdapter = new BinaryAdapter(reader);
      const result = AlgebraicValue.deserialize(
        AlgebraicType.createStringType(),
        adapter
      );

      expect(result.asString()).toEqual("zażółć gęślą jaźń");
    });
  });

  describe("deserialize with a JSON adapter", () => {
    test("should correctly deserialize array with U8 type", () => {
      const value = "0002FF";
      const adapter: JSONAdapter = new JSONAdapter(value);
      const type = AlgebraicType.createBytesType();
      const result = AlgebraicValue.deserialize(type, adapter);

      expect(result.asBytes()).toEqual(new Uint8Array([0, 2, 255]));
    });

    test("should correctly deserialize array with U128 type", () => {
      const u128_max = BigInt(2) ** BigInt(128) - BigInt(1);
      const value = [BigInt(1), u128_max, BigInt(10)];
      const adapter: JSONAdapter = new JSONAdapter(value);
      const type = AlgebraicType.createArrayType(AlgebraicType.createU128Type());
      const result = AlgebraicValue.deserialize(type, adapter);

      expect(result.asJsArray("BigInt")).toEqual([
        BigInt(1),
        u128_max,
        BigInt(10),
      ]);
    });

    test("should correctly deserialize an U128 type", () => {
      const value = BigInt("123456789123456789");
      const adapter: JSONAdapter = new JSONAdapter(value);
      const result = AlgebraicValue.deserialize(
        AlgebraicType.createU128Type(),
        adapter
      );

      expect(result.asBigInt()).toEqual(value);
    });

    test("should correctly deserialize a boolean type", () => {
      const adapter: JSONAdapter = new JSONAdapter(true);
      const result = AlgebraicValue.deserialize(
        AlgebraicType.createBoolType(),
        adapter
      );

      expect(result.asBool()).toEqual(true);
    });

    test("should correctly deserialize a string type", () => {
      const text = "zażółć gęślą jaźń";
      const adapter: JSONAdapter = new JSONAdapter(text);
      const result = AlgebraicValue.deserialize(
        AlgebraicType.createStringType(),
        adapter
      );

      expect(result.asString()).toEqual("zażółć gęślą jaźń");
    });
  });
});
