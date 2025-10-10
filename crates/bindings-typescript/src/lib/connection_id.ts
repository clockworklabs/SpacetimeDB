import { AlgebraicType } from './algebraic_type';
import { hexStringToU128, u128ToHexString, u128ToUint8Array } from './utils';

export type ConnectionIdAlgebraicType = {
  tag: 'Product';
  value: {
    elements: [{ name: '__connection_id__'; algebraicType: { tag: 'U128' } }];
  };
};

/**
 * A unique identifier for a client connected to a database.
 */
export class ConnectionId {
  __connection_id__: bigint;

  /**
   * Creates a new `ConnectionId`.
   */
  constructor(data: bigint) {
    this.__connection_id__ = data;
  }

  /**
   * Get the algebraic type representation of the {@link ConnectionId} type.
   * @returns The algebraic type representation of the type.
   */
  static getAlgebraicType(): ConnectionIdAlgebraicType {
    return AlgebraicType.Product({
      elements: [
        { name: '__connection_id__', algebraicType: AlgebraicType.U128 },
      ],
    });
  }

  isZero(): boolean {
    return this.__connection_id__ === BigInt(0);
  }

  static nullIfZero(addr: ConnectionId): ConnectionId | null {
    if (addr.isZero()) {
      return null;
    } else {
      return addr;
    }
  }

  static random(): ConnectionId {
    function randomU8(): number {
      return Math.floor(Math.random() * 0xff);
    }
    let result = BigInt(0);
    for (let i = 0; i < 16; i++) {
      result = (result << BigInt(8)) | BigInt(randomU8());
    }
    return new ConnectionId(result);
  }

  /**
   * Compare two connection IDs for equality.
   */
  isEqual(other: ConnectionId): boolean {
    return this.__connection_id__ == other.__connection_id__;
  }

  /**
   * Print the connection ID as a hexadecimal string.
   */
  toHexString(): string {
    return u128ToHexString(this.__connection_id__);
  }

  /**
   * Convert the connection ID to a Uint8Array.
   */
  toUint8Array(): Uint8Array {
    return u128ToUint8Array(this.__connection_id__);
  }

  /**
   * Parse a connection ID from a hexadecimal string.
   */
  static fromString(str: string): ConnectionId {
    return new ConnectionId(hexStringToU128(str));
  }

  static fromStringOrNull(str: string): ConnectionId | null {
    const addr = ConnectionId.fromString(str);
    if (addr.isZero()) {
      return null;
    } else {
      return addr;
    }
  }
}
