/**
 * A unique identifier for a client connected to a database.
 */
export class Address {
  #data: Uint8Array;

  get __address_bytes(): Uint8Array {
    return this.toUint8Array();
  }

  /**
   * Creates a new `Address`.
   */
  constructor(data: Uint8Array) {
    this.#data = data;
  }

  #isZero(): boolean {
    return this.#data.every(b => b == 0);
  }

  static nullIfZero(addr: Address): Address | null {
    if (addr.#isZero()) {
      return null;
    } else {
      return addr;
    }
  }

  static random(): Address {
    function randomByte(): number {
      return Math.floor(Math.random() * 255);
    }
    let data = new Uint8Array(16);
    for (let i = 0; i < 16; i++) {
      data[i] = randomByte();
    }
    return new Address(data);
  }

  /**
   * Compare two addresses for equality.
   */
  isEqual(other: Address): boolean {
    if (this.#data.length !== other.#data.length) {
      return false;
    }
    for (let i = 0; i < this.#data.length; i++) {
      if (this.#data[i] !== other.#data[i]) {
        return false;
      }
    }
    return true;
  }

  /**
   * Print the address as a hexadecimal string.
   */
  toHexString(): string {
    return Array.prototype.map
      .call(this.#data, x => ('00' + x.toString(16)).slice(-2))
      .join('');
  }

  toUint8Array(): Uint8Array {
    return this.#data;
  }

  /**
   * Parse an Address from a hexadecimal string.
   */
  static fromString(str: string): Address {
    let matches = str.match(/.{1,2}/g) || [];
    let data = Uint8Array.from(
      matches.map((byte: string) => parseInt(byte, 16))
    );
    return new Address(data);
  }

  static fromStringOrNull(str: string): Address | null {
    let addr = Address.fromString(str);
    if (addr.#isZero()) {
      return null;
    } else {
      return addr;
    }
  }
}
