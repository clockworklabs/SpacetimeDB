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

type UuidVersion = 'Nil' | 'V4' | 'V7';

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
  static fromUnixMillisV7(
    millisSinceUnixEpoch: bigint,
    counterRandomBytes: Uint8Array
  ): Uuid {
    if (counterRandomBytes.length !== 10) {
      throw new Error('UUIDv7 requires 10 random bytes');
    }
    // Translated from Rust `uuid`
    const millis = millisSinceUnixEpoch;

    const millisHigh = Number((millis >> 16n) & 0xffff_ffffn);
    const millisLow = Number(millis & 0xffffn);

    const counterRandom =
      (((counterRandomBytes[0] << 8) | counterRandomBytes[1]) & 0x0fff) |
      (0x7 << 12);

    const bytes = new Uint8Array(16);

    bytes[0] = (millisHigh >>> 24) & 0xff;
    bytes[1] = (millisHigh >>> 16) & 0xff;
    bytes[2] = (millisHigh >>> 8) & 0xff;
    bytes[3] = millisHigh & 0xff;

    bytes[4] = (millisLow >>> 8) & 0xff;
    bytes[5] = millisLow & 0xff;

    bytes[6] = (counterRandom >>> 8) & 0xff;
    bytes[7] = counterRandom & 0xff;

    bytes[8] = (counterRandomBytes[2] & 0x3f) | 0x80;
    bytes[9] = counterRandomBytes[3];
    bytes[10] = counterRandomBytes[4];
    bytes[11] = counterRandomBytes[5];
    bytes[12] = counterRandomBytes[6];
    bytes[13] = counterRandomBytes[7];
    bytes[14] = counterRandomBytes[8];
    bytes[15] = counterRandomBytes[9];

    return new Uuid(Uuid.bytesToBigInt(bytes));
  }

  /** Generate a UUIDv7 using a monotonic clock generator. */
  static fromClockV7(clock: ClockGenerator, random: Uint8Array): Uuid {
    const millis = clock.tick().toMillis();
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

  /**
   * Returns the version of this UUID.
   */
  getVersion(): UuidVersion {
    const version = (this.toBytes()[6] >> 4) & 0x0f;

    switch (version) {
      case 4:
        return 'V4';
      case 7:
        return 'V7';
      default:
        if (this == Uuid.NIL) {
          return 'Nil';
        }
        throw new Error(`Unsupported UUID version: ${version}`);
    }
  }

  compareTo(other: Uuid): number {
    if (this.__uuid__ < other.__uuid__) return -1;
    if (this.__uuid__ > other.__uuid__) return 1;

    return 0;
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
