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

type UuidVersion = 'Nil' | 'V4' | 'V7' | 'Max';

/** 128-bit UUID abstraction */
export class Uuid {
  __uuid__: bigint;

  /** The nil UUID (all zeros). */
  static readonly NIL = new Uuid(0n);
  static readonly MAX_UUID_BIGINT = 0xffffffffffffffffffffffffffffffffn;
  /** The maximum UUID (all ones). */
  static readonly MAX = new Uuid(Uuid.MAX_UUID_BIGINT);

  /** Create a UUID from a bigint (u128).
   *  @throws Error if the bigint is not between 0 and `MAX_UUID_BIGINT`.
   * */
  constructor(u: bigint) {
    // Must fit in exactly 16 bytes
    if (u < 0n || u > Uuid.MAX_UUID_BIGINT) {
      throw new Error('Invalid UUID: must be between 0 and `MAX_UUID_BIGINT`');
    }
    this.__uuid__ = u;
  }

  /** Create a UUIDv4 from explicit random bytes.
   * @throws Error if the byte array is not exactly 16 bytes.
   * */
  static fromRandomBytesV4(bytes: Uint8Array): Uuid {
    if (bytes.length !== 16) throw new Error('UUIDv4 requires 16 bytes');
    const arr = new Uint8Array(bytes);
    arr[6] = (arr[6] & 0x0f) | 0x40; // version 4
    arr[8] = (arr[8] & 0x3f) | 0x80; // variant
    return new Uuid(Uuid.bytesToBigInt(arr));
  }

  /** Create a UUIDv7 from a UNIX timestamp (milliseconds) and 10 random bytes.
   * @throws Error if the byte array is not exactly 10 bytes.
   * */
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

  /** Generate a UUIDv7 using a monotonic clock generator.
   * @throws Error if the random byte array is not exactly 10 bytes.
   * */
  static fromClockV7(clock: ClockGenerator, random: Uint8Array): Uuid {
    const millis = clock.tick().toMillis();
    return Uuid.fromUnixMillisV7(millis, random);
  }

  /** Parse a UUID from a string.
   * @throws Error if the string is not a valid UUID format.
   * */
  static parse(s: string): Uuid {
    const hex = s.replace(/-/g, '');
    if (hex.length !== 32) throw new Error('Invalid hex UUID');

    let v = 0n;
    for (let i = 0; i < 32; i += 2) {
      v = (v << 8n) | BigInt(parseInt(hex.slice(i, i + 2), 16));
    }
    return new Uuid(v);
  }

  /** Convert to string (hyphenated form). */
  toString(): string {
    const bytes = Uuid.bigIntToBytes(this.__uuid__);
    const hex = [...bytes].map(b => b.toString(16).padStart(2, '0')).join('');

    // Format as 8-4-4-4-12
    return (
      hex.slice(0, 8) +
      '-' +
      hex.slice(8, 12) +
      '-' +
      hex.slice(12, 16) +
      '-' +
      hex.slice(16, 20) +
      '-' +
      hex.slice(20)
    );
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
   * @throws Error if the UUID version is unsupported.
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
        if (this == Uuid.MAX) {
          return 'Max';
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
