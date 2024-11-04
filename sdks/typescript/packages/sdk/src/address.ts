import { hexStringToU128, u128ToHexString, u128ToUint8Array } from './utils';

/**
 * A unique identifier for a client connected to a database.
 */
export class Address {
  data: bigint;

  get __address__(): bigint {
    return this.data;
  }

  /**
   * Creates a new `Address`.
   */
  constructor(data: bigint) {
    this.data = data;
  }

  isZero(): boolean {
    return this.data === BigInt(0);
  }

  static nullIfZero(addr: Address): Address | null {
    if (addr.isZero()) {
      return null;
    } else {
      return addr;
    }
  }

  static random(): Address {
    function randomU8(): number {
      return Math.floor(Math.random() * 0xff);
    }
    let result = BigInt(0);
    for (let i = 0; i < 16; i++) {
      result = (result << BigInt(8)) | BigInt(randomU8());
    }
    return new Address(result);
  }

  /**
   * Compare two addresses for equality.
   */
  isEqual(other: Address): boolean {
    return this.data == other.data;
  }

  /**
   * Print the address as a hexadecimal string.
   */
  toHexString(): string {
    return u128ToHexString(this.data);
  }

  /**
   * Convert the address to a Uint8Array.
   */
  toUint8Array(): Uint8Array {
    return u128ToUint8Array(this.data);
  }

  /**
   * Parse an Address from a hexadecimal string.
   */
  static fromString(str: string): Address {
    return new Address(hexStringToU128(str));
  }

  static fromStringOrNull(str: string): Address | null {
    let addr = Address.fromString(str);
    if (addr.isZero()) {
      return null;
    } else {
      return addr;
    }
  }
}
