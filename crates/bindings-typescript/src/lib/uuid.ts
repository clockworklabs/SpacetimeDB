import { Timestamp } from './timestamp';
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

/**
 * Supported UUID versions.
 *
 * - `Nil` – The "Nil" UUID (all zeros)
 * - `V4`  – Version 4: random
 * - `V7`  – Version 7: timestamp + counter + random
 * - `Max` – The "Max" UUID (all ones)
 */
type UuidVersion = 'Nil' | 'V4' | 'V7' | 'Max';

/**
 * A universally unique identifier (UUID).
 *
 * Supports UUID `Nil`, `Max`, `V4` (random), and `V7`
 * (timestamp + counter + random).
 *
 * Internally represented as an unsigned 128-bit between 0 and `MAX_UUID_BIGINT`.
 */
export class Uuid {
  __uuid__: bigint;

  /**
   * The nil UUID (all zeros).
   *
   * @example
   * ```ts
   * const uuid = Uuid.NIL;
   * console.assert(
   *   uuid.toString() === "00000000-0000-0000-0000-000000000000"
   * );
   * ```
   */
  static readonly NIL = new Uuid(0n);
  static readonly MAX_UUID_BIGINT = 0xffffffffffffffffffffffffffffffffn;
  /**
   * The max UUID (all ones).
   *
   * @example
   * ```ts
   * const uuid = Uuid.MAX;
   * console.assert(
   *   uuid.toString() === "ffffffff-ffff-ffff-ffff-ffffffffffff"
   * );
   * ```
   */
  static readonly MAX = new Uuid(Uuid.MAX_UUID_BIGINT);

  /**
   * Create a UUID from a raw 128-bit value.
   *
   * @param u - Unsigned 128-bit integer
   * @throws {Error} If the value is outside the valid UUID range
   */
  constructor(u: bigint) {
    // Must fit in exactly 16 bytes
    if (u < 0n || u > Uuid.MAX_UUID_BIGINT) {
      throw new Error('Invalid UUID: must be between 0 and `MAX_UUID_BIGINT`');
    }
    this.__uuid__ = u;
  }

  /**
   * Create a UUID `v4` from explicit random bytes.
   *
   * This method assumes the bytes are already sufficiently random.
   * It only sets the appropriate bits for the UUID version and variant.
   *
   * @param bytes - Exactly 16 random bytes
   * @returns A UUID `v4`
   * @throws {Error} If `bytes.length !== 16`
   *
   * @example
   * ```ts
   * const randomBytes = new Uint8Array(16);
   * const uuid = Uuid.fromRandomBytesV4(randomBytes);
   *
   * console.assert(
   *   uuid.toString() === "00000000-0000-4000-8000-000000000000"
   * );
   * ```
   */
  static fromRandomBytesV4(bytes: Uint8Array): Uuid {
    if (bytes.length !== 16) throw new Error('UUID v4 requires 16 bytes');
    const arr = new Uint8Array(bytes);
    arr[6] = (arr[6] & 0x0f) | 0x40; // version 4
    arr[8] = (arr[8] & 0x3f) | 0x80; // variant
    return new Uuid(Uuid.bytesToBigInt(arr));
  }

  /**
   * Generate a UUID `v7` using a monotonic counter from `0` to `2^31 - 1`,
   * a timestamp, and 4 random bytes.
   *
   * The counter wraps around on overflow.
   *
   * The UUID `v7` is structured as follows:
   *
   * ```ascii
   * ┌───────────────────────────────────────────────┬───────────────────┐
   * | B0  | B1  | B2  | B3  | B4  | B5              |         B6        |
   * ├───────────────────────────────────────────────┼───────────────────┤
   * |                 unix_ts_ms                    |      version 7    |
   * └───────────────────────────────────────────────┴───────────────────┘
   * ┌──────────────┬─────────┬──────────────────┬───────────────────────┐
   * | B7           | B8      | B9  | B10 | B11  | B12 | B13 | B14 | B15 |
   * ├──────────────┼─────────┼──────────────────┼───────────────────────┤
   * | counter_high | variant |    counter_low   |        random         |
   * └──────────────┴─────────┴──────────────────┴───────────────────────┘
   * ```
   *
   * @param counter - Mutable monotonic counter (31-bit)
   * @param now - Timestamp since the Unix epoch
   * @param randomBytes - Exactly 4 random bytes
   * @returns A UUID `v7`
   *
   * @throws {Error} If the `counter` is negative
   * @throws {Error} If the `timestamp` is before the Unix epoch
   * @throws {Error} If `randomBytes.length !== 4`
   *
   * @example
   * ```ts
   * const now = Timestamp.fromMillis(1_686_000_000_000n);
   * const counter = { value: 1 };
   * const randomBytes = new Uint8Array(4);
   *
   * const uuid = Uuid.fromCounterV7(counter, now, randomBytes);
   *
   * console.assert(
   *   uuid.toString() === "0000647e-5180-7000-8000-000200000000"
   * );
   * ```
   */
  static fromCounterV7(
    counter: { value: number },
    now: Timestamp,
    randomBytes: Uint8Array
  ): Uuid {
    if (randomBytes.length !== 4) {
      throw new Error('`fromCounterV7` requires `randomBytes.length == 4`');
    }

    if (counter.value < 0) {
      throw new Error('`fromCounterV7` uuid `counter` must be non-negative');
    }

    if (now.__timestamp_micros_since_unix_epoch__ < 0) {
      throw new Error('`fromCounterV7` `timestamp` before unix epoch');
    }

    // 31-bit monotonic counter with wraparound
    const counterVal = counter.value;
    counter.value = (counterVal + 1) & 0x7fff_ffff;

    // 48-bit unix timestamp (ms)
    const tsMs = now.toMillis() & 0xffff_ffff_ffffn;

    const bytes = new Uint8Array(16);

    // unix_ts_ms (48 bits)
    bytes[0] = Number((tsMs >> 40n) & 0xffn);
    bytes[1] = Number((tsMs >> 32n) & 0xffn);
    bytes[2] = Number((tsMs >> 24n) & 0xffn);
    bytes[3] = Number((tsMs >> 16n) & 0xffn);
    bytes[4] = Number((tsMs >> 8n) & 0xffn);
    bytes[5] = Number(tsMs & 0xffn);

    // Counter bits (31 bits total)
    bytes[7] = (counterVal >>> 23) & 0xff;
    bytes[9] = (counterVal >>> 15) & 0xff;
    bytes[10] = (counterVal >>> 7) & 0xff;
    bytes[11] = ((counterVal & 0x7f) << 1) & 0xff;

    // Random bytes
    bytes[12] |= randomBytes[0] & 0x7f;
    bytes[13] = randomBytes[1];
    bytes[14] = randomBytes[2];
    bytes[15] = randomBytes[3];

    // Version 7
    bytes[6] = (bytes[6] & 0x0f) | 0x70;

    // Variant RFC4122
    bytes[8] = (bytes[8] & 0x3f) | 0x80;

    return new Uuid(Uuid.bytesToBigInt(bytes));
  }

  /**
   * Parse a UUID from a string representation.
   *
   * @param s - UUID string
   * @returns Parsed UUID
   * @throws {Error} If the string is not a valid UUID
   *
   * @example
   * ```ts
   * const s = "01888d6e-5c00-7000-8000-000000000000";
   * const uuid = Uuid.parse(s);
   *
   * console.assert(uuid.toString() === s);
   * ```
   */
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
   *
   * This represents the algorithm used to generate the value.
   *
   * @returns A `UuidVersion`
   * @throws {Error} If the version field is not recognized
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

  /**
   * Extract the monotonic counter from a UUIDv7.
   *
   * Intended for testing and diagnostics.
   * Behavior is undefined if called on a non-V7 UUID.
   *
   * @returns 31-bit counter value
   */
  getCounter(): number {
    const bytes = this.toBytes(); // big-endian, 16 bytes

    const high = bytes[7]; // bits 30..23
    const mid1 = bytes[9]; // bits 22..15
    const mid2 = bytes[10]; // bits 14..7
    const low = bytes[11] >>> 1; // bits 6..0

    // reconstruct 31-bit counter
    return (high << 23) | (mid1 << 15) | (mid2 << 7) | low | 0; // force 32-bit int
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
