import { describe, expect, test } from 'vitest';
import {
  AlgebraicType,
  BinaryReader,
  BinaryWriter,
  ConnectionId,
  Identity,
  ScheduleAt,
  Uuid,
} from '../src';

describe('it correctly serializes and deserializes algebraic values', () => {
  test('when it serializes and deserializes with a product type', () => {
    const value = { foo: 'foobar' };
    const algebraic_type = AlgebraicType.Product({
      elements: [{ name: 'foo', algebraicType: AlgebraicType.String }],
    });
    const binaryWriter = new BinaryWriter(1024);
    AlgebraicType.serializeValue(binaryWriter, algebraic_type, value);

    const buffer = binaryWriter.getBuffer();

    expect(buffer).toEqual(
      new Uint8Array([6, 0, 0, 0, 102, 111, 111, 98, 97, 114])
    );

    const deserializedValue = AlgebraicType.deserializeValue(
      new BinaryReader(buffer),
      algebraic_type
    );

    expect(deserializedValue).toEqual(value);
  });

  test('when it serializes and deserializes with a sum type', () => {
    const value = { tag: 'bar', value: 5 };
    const algebraic_type = AlgebraicType.Sum({
      variants: [
        { name: 'bar', algebraicType: AlgebraicType.U32 },
        { name: 'foo', algebraicType: AlgebraicType.String },
      ],
    });
    const binaryWriter = new BinaryWriter(1024);
    AlgebraicType.serializeValue(binaryWriter, algebraic_type, value);

    const buffer = binaryWriter.getBuffer();

    expect(buffer).toEqual(new Uint8Array([0, 5, 0, 0, 0]));

    const deserializedValue = AlgebraicType.deserializeValue(
      new BinaryReader(buffer),
      algebraic_type
    );

    expect(deserializedValue).toEqual(value);
  });

  test('when it serializes and deserializes an Identity type', () => {
    const value = {
      __identity__: BigInt(1234567890123456789012345678901234567890n),
    };

    const algebraic_type = Identity.getAlgebraicType();
    const binaryWriter = new BinaryWriter(1024);
    AlgebraicType.serializeValue(binaryWriter, algebraic_type, value);

    const buffer = binaryWriter.getBuffer();

    // Little endian encoding of the number 1234567890123456789012345678901234567890n
    expect(buffer).toEqual(
      new Uint8Array([
        210, 10, 63, 206, 150, 95, 188, 172, 184, 243, 219, 192, 117, 32, 201,
        160, 3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
      ])
    );

    const deserializedValue = AlgebraicType.deserializeValue(
      new BinaryReader(buffer),
      algebraic_type
    );

    expect(deserializedValue).toEqual(value);
  });

  test('when it serializes and deserializes an Interval ScheduleAt', () => {
    const value = {
      tag: 'Interval',
      value: {
        __time_duration_micros__: BigInt(1234567890123456789n),
      },
    };

    const algebraic_type = ScheduleAt.getAlgebraicType();
    const binaryWriter = new BinaryWriter(1024);
    AlgebraicType.serializeValue(binaryWriter, algebraic_type, value);

    const buffer = binaryWriter.getBuffer();
    expect(buffer).toEqual(
      new Uint8Array([0, 21, 129, 233, 125, 244, 16, 34, 17])
    );

    const deserializedValue = AlgebraicType.deserializeValue(
      new BinaryReader(buffer),
      algebraic_type
    );

    expect(deserializedValue).toEqual(value);
  });

  test('when it serializes and deserializes a Time ScheduleAt', () => {
    const value = {
      tag: 'Time',
      value: {
        __timestamp_micros_since_unix_epoch__: BigInt(1234567890123456789n),
      },
    };

    const algebraic_type = ScheduleAt.getAlgebraicType();
    const binaryWriter = new BinaryWriter(1024);
    AlgebraicType.serializeValue(binaryWriter, algebraic_type, value);

    const buffer = binaryWriter.getBuffer();
    expect(buffer).toEqual(
      new Uint8Array([1, 21, 129, 233, 125, 244, 16, 34, 17])
    );

    const deserializedValue = AlgebraicType.deserializeValue(
      new BinaryReader(buffer),
      algebraic_type
    );

    expect(deserializedValue).toEqual(value);
  });

  test('when it serializes and deserializes a ConnectionId ', () => {
    const U128_MAX = (1n << 128n) - 1n;
    const value = {
      __connection_id__: U128_MAX,
    };

    const algebraic_type = ConnectionId.getAlgebraicType();
    const binaryWriter = new BinaryWriter(1024);
    AlgebraicType.serializeValue(binaryWriter, algebraic_type, value);

    const buffer = binaryWriter.getBuffer();
    expect(buffer).toEqual(
      new Uint8Array([
        255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255, 255,
        255, 255,
      ])
    );

    const deserializedValue = AlgebraicType.deserializeValue(
      new BinaryReader(buffer),
      algebraic_type
    );

    console.log(deserializedValue);

    expect(deserializedValue).toEqual(value);
  });

  test('when it serializes and deserializes an Uuid ', () => {
    const value = {
      __uuid__: BigInt('0x1234567890abcdef1234567890abcdef'),
    };

    const algebraic_type = Uuid.getAlgebraicType();
    const binaryWriter = new BinaryWriter(1024);
    AlgebraicType.serializeValue(binaryWriter, algebraic_type, value);

    const buffer = binaryWriter.getBuffer();
    expect(buffer).toEqual(
      new Uint8Array([
        239, 205, 171, 144, 120, 86, 52, 18, 239, 205, 171, 144, 120, 86, 52,
        18,
      ])
    );

    const deserializedValue = AlgebraicType.deserializeValue(
      new BinaryReader(buffer),
      algebraic_type
    );

    expect(deserializedValue).toEqual(value);
  });
});
