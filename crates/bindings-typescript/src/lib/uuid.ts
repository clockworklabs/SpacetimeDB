import { parse as uuidParse, stringify as uuidStringify } from 'uuid';
import { ClockGenerator } from './timestamp';
import { AlgebraicType } from './algebraic_type.ts';

export type UuidAlgebraicType = {
  tag: 'Product';
  value: {
    elements: [
      {
        name: '__uuid__';
        algebraicType: { tag: 'U128' };
      },
    ];
  };
};

/** 128-bit UUID abstraction */
export class Uuid {
  __uuid__: bigint;

  constructor(u: bigint) {
    this.__uuid__ = u;
  }

  /** The nil UUID (all zeros). */
  static readonly NIL = new Uuid(0n);

  /** Create a UUIDv4 from explicit random bytes. */
  static fromRandomBytesV4(bytes: Uint8Array): Uuid {
    if (bytes.length !== 16) throw new Error('UUIDv4 requires 16 bytes');
    const arr = new Uint8Array(bytes);
    arr[6] = (arr[6] & 0x0f) | 0x40; // version 4
    arr[8] = (arr[8] & 0x3f) | 0x80; // variant
    return new Uuid(Uuid.bytesToBigInt(arr));
  }

  /** Create a UUIDv7 from a UNIX timestamp (milliseconds) and 10 random bytes. */
  static fromUnixMillisV7(millis: number | bigint, random: Uint8Array): Uuid {
    if (random.length !== 10)
      throw new Error('UUIDv7 requires 10 random bytes');

    const buf = new Uint8Array(16);
    const ts = BigInt(millis);
    // encode 48-bit timestamp (6 bytes)
    for (let i = 0; i < 6; i++)
      buf[5 - i] = Number((ts >> BigInt(i * 8)) & 0xffn);
    buf.set(random, 6);
    buf[6] = (buf[6] & 0x0f) | 0x70; // version 7
    buf[8] = (buf[8] & 0x3f) | 0x80; // variant

    return new Uuid(Uuid.bytesToBigInt(buf));
  }

  /** Generate a UUIDv7 using a monotonic clock generator. */
  static fromClockV7(clock: ClockGenerator, random: Uint8Array): Uuid {
    const ts = clock.tick();
    const millis = ts.toMillis();
    return Uuid.fromUnixMillisV7(millis, random);
  }

  /** Parse a UUID from a string. */
  static parse(s: string): Uuid {
    const bytes = uuidParse(s);
    return new Uuid(Uuid.bytesToBigInt(bytes));
  }

  /** Convert to string (hyphenated form). */
  toString(): string {
    return uuidStringify(Uuid.bigIntToBytes(this.__uuid__));
  }

  /** Convert to bigint (u128). */
  asBigInt(): bigint {
    return this.__uuid__;
  }

  /** Return a `Uint8Array` of 16 bytes. */
  toBytes(): Uint8Array {
    return Uuid.bigIntToBytes(this.__uuid__);
  }

  private static bytesToBigInt(bytes: Uint8Array): bigint {
    let result = 0n;
    for (const b of bytes) result = (result << 8n) | BigInt(b);
    return result;
  }

  private static bigIntToBytes(value: bigint): Uint8Array {
    const bytes = new Uint8Array(16);
    for (let i = 15; i >= 0; i--) {
      bytes[i] = Number(value & 0xffn);
      value >>= 8n;
    }
    return bytes;
  }

  static getAlgebraicType(): UuidAlgebraicType {
    return AlgebraicType.Product({
      elements: [
        {
          name: '__uuid__',
          algebraicType: AlgebraicType.U128,
        },
      ],
    });
  }
}
