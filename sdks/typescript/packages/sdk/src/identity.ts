import BinaryReader from './binary_reader';
import BinaryWriter from './binary_writer';
import { hexStringToU256, u256ToHexString, u256ToUint8Array } from './utils';

/**
 * A unique identifier for a user connected to a database.
 */
export class Identity {
  data: bigint;

  get __identity__(): bigint {
    return this.data;
  }

  /**
   * Creates a new `Identity`.
   *
   * `data` can be a hexadecimal string or a `bigint`.
   */
  constructor(data: string | bigint) {
    // we get a JSON with __identity__ when getting a token with a JSON API
    // and an bigint when using BSATN
    this.data = typeof data === 'string' ? hexStringToU256(data) : data;
  }

  /**
   * Compare two identities for equality.
   */
  isEqual(other: Identity): boolean {
    return this.toHexString() === other.toHexString();
  }

  /**
   * Print the identity as a hexadecimal string.
   */
  toHexString(): string {
    return u256ToHexString(this.data);
  }

  /**
   * Convert the address to a Uint8Array.
   */
  toUint8Array(): Uint8Array {
    return u256ToUint8Array(this.data);
  }

  /**
   * Parse an Identity from a hexadecimal string.
   */
  static fromString(str: string): Identity {
    return new Identity(str);
  }
}
