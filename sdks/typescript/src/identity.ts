export class Identity {
  private data: Uint8Array;

  constructor(data: Uint8Array) {
    this.data = data;
  }

  // Method to compare two identities
  isEqual(other: Identity): boolean {
    if (this.data.length !== other.data.length) {
      return false;
    }
    for (let i = 0; i < this.data.length; i++) {
      if (this.data[i] !== other.data[i]) {
        return false;
      }
    }
    return true;
  }

  // Method to convert the Uint8Array to a hex string
  toHexString(): string {
    return Array.prototype.map
      .call(this.data, (x) => ("00" + x.toString(16)).slice(-2))
      .join("");
  }

  toUint8Array(): Uint8Array {
    return this.data;
  }

  // Static method to create an Identity from a hex string
  static fromString(str: string): Identity {
    let matches = str.match(/.{1,2}/g) || [];
    let data = Uint8Array.from(
      matches.map((byte: string) => parseInt(byte, 16))
    );
    return new Identity(data);
  }
}
