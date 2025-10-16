import { AlgebraicType } from './algebraic_type';
import { hexStringToU256, u256ToHexString, u256ToUint8Array } from './utils';

export type IdentityAlgebraicType = {
  tag: 'Product';
  value: {
    elements: [{ name: '__identity__'; algebraicType: { tag: 'U256' } }];
  };
};

/**
 * A unique identifier for a user connected to a database.
 */
export class Identity {
  __identity__: bigint;

  /**
   * Creates a new `Identity`.
   *
   * `data` can be a hexadecimal string or a `bigint`.
   */
  constructor(data: string | bigint) {
    // we get a JSON with __identity__ when getting a token with a JSON API
    // and an bigint when using BSATN
    this.__identity__ = typeof data === 'string' ? hexStringToU256(data) : data;
  }

  /**
   * Get the algebraic type representation of the {@link Identity} type.
   * @returns The algebraic type representation of the type.
   */
  static getAlgebraicType(): IdentityAlgebraicType {
    return AlgebraicType.Product({
      elements: [{ name: '__identity__', algebraicType: AlgebraicType.U256 }],
    });
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
    return u256ToHexString(this.__identity__);
  }

  /**
   * Convert the address to a Uint8Array.
   */
  toUint8Array(): Uint8Array {
    return u256ToUint8Array(this.__identity__);
  }

  /**
   * Parse an Identity from a hexadecimal string.
   */
  static fromString(str: string): Identity {
    return new Identity(str);
  }

  /**
   * Zero identity (0x0000000000000000000000000000000000000000000000000000000000000000)
   */
  static zero(): Identity {
    return new Identity(0n);
  }

  toString(): string {
    return this.toHexString();
  }
}
