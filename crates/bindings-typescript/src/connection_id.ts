import { hexStringToU128, u128ToHexString, u128ToUint8Array } from './utils';

/**
 * A unique identifier for a client connected to a database.
 */
export class ConnectionId {
  data: bigint;

  get __connection_id__(): bigint {
    return this.data;
  }

  /**
   * Creates a new `ConnectionId`.
   */
  constructor(data: bigint) {
    this.data = data;
  }

  isZero(): boolean {
    return this.data === BigInt(0);
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
    return this.data == other.data;
  }

  /**
   * Print the connection ID as a hexadecimal string.
   */
  toHexString(): string {
    return u128ToHexString(this.data);
  }

  /**
   * Convert the connection ID to a Uint8Array.
   */
  toUint8Array(): Uint8Array {
    return u128ToUint8Array(this.data);
  }

  /**
   * Parse a connection ID from a hexadecimal string.
   */
  static fromString(str: string): ConnectionId {
    return new ConnectionId(hexStringToU128(str));
  }

  static fromStringOrNull(str: string): ConnectionId | null {
    let addr = ConnectionId.fromString(str);
    if (addr.isZero()) {
      return null;
    } else {
      return addr;
    }
  }
}
