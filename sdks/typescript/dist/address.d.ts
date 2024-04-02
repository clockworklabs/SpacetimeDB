/**
 * A unique public identifier for a client connected to a database.
 */
export declare class Address {
  private data;
  /**
   * Creates a new `Address`.
   */
  constructor(data: Uint8Array);
  private isZero;
  static nullIfZero(data: Uint8Array): Address | null;
  static random(): Address;
  /**
   * Compare two addresses for equality.
   */
  isEqual(other: Address): boolean;
  /**
   * Print the address as a hexadecimal string.
   */
  toHexString(): string;
  toUint8Array(): Uint8Array;
  /**
   * Parse an Address from a hexadecimal string.
   */
  static fromString(str: string): Address;
  static fromStringOrNull(str: string): Address | null;
}
