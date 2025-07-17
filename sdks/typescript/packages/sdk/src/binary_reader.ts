export default class BinaryReader {
  #buffer: DataView;
  #offset: number = 0;

  constructor(input: Uint8Array) {
    this.#buffer = new DataView(input.buffer);
    this.#offset = input.byteOffset;
  }

  get offset(): number {
    return this.#offset;
  }

  readUInt8Array(): Uint8Array {
    const length = this.readU32();
    const value: Uint8Array = new Uint8Array(
      this.#buffer.buffer,
      this.#offset,
      length
    );
    this.#offset += length;
    return value;
  }

  readBool(): boolean {
    const value = this.#buffer.getUint8(this.#offset);
    this.#offset += 1;
    return value !== 0;
  }

  readByte(): number {
    const value = this.#buffer.getUint8(this.#offset);
    this.#offset += 1;
    return value;
  }

  readBytes(length: number): Uint8Array {
    const value: DataView = new DataView(
      this.#buffer.buffer,
      this.#offset,
      length
    );
    this.#offset += length;
    return new Uint8Array(value.buffer);
  }

  readI8(): number {
    const value = this.#buffer.getInt8(this.#offset);
    this.#offset += 1;
    return value;
  }

  readU8(): number {
    const value = this.#buffer.getUint8(this.#offset);
    this.#offset += 1;
    return value;
  }

  readI16(): number {
    const value = this.#buffer.getInt16(this.#offset, true);
    this.#offset += 2;
    return value;
  }

  readU16(): number {
    const value = this.#buffer.getUint16(this.#offset, true);
    this.#offset += 2;
    return value;
  }

  readI32(): number {
    const value = this.#buffer.getInt32(this.#offset, true);
    this.#offset += 4;
    return value;
  }

  readU32(): number {
    const value = this.#buffer.getUint32(this.#offset, true);
    this.#offset += 4;
    return value;
  }

  readI64(): bigint {
    const value = this.#buffer.getBigInt64(this.#offset, true);
    this.#offset += 8;
    return value;
  }

  readU64(): bigint {
    const value = this.#buffer.getBigUint64(this.#offset, true);
    this.#offset += 8;
    return value;
  }

  readU128(): bigint {
    const lowerPart = this.#buffer.getBigUint64(this.#offset, true);
    const upperPart = this.#buffer.getBigUint64(this.#offset + 8, true);
    this.#offset += 16;

    return (upperPart << BigInt(64)) + lowerPart;
  }

  readI128(): bigint {
    const lowerPart = this.#buffer.getBigUint64(this.#offset, true);
    const upperPart = this.#buffer.getBigInt64(this.#offset + 8, true);
    this.#offset += 16;

    return (upperPart << BigInt(64)) + lowerPart;
  }

  readU256(): bigint {
    const p0 = this.#buffer.getBigUint64(this.#offset, true);
    const p1 = this.#buffer.getBigUint64(this.#offset + 8, true);
    const p2 = this.#buffer.getBigUint64(this.#offset + 16, true);
    const p3 = this.#buffer.getBigUint64(this.#offset + 24, true);
    this.#offset += 32;

    return (
      (p3 << BigInt(3 * 64)) +
      (p2 << BigInt(2 * 64)) +
      (p1 << BigInt(1 * 64)) +
      p0
    );
  }

  readI256(): bigint {
    const p0 = this.#buffer.getBigUint64(this.#offset, true);
    const p1 = this.#buffer.getBigUint64(this.#offset + 8, true);
    const p2 = this.#buffer.getBigUint64(this.#offset + 16, true);
    const p3 = this.#buffer.getBigInt64(this.#offset + 24, true);
    this.#offset += 32;

    return (
      (p3 << BigInt(3 * 64)) +
      (p2 << BigInt(2 * 64)) +
      (p1 << BigInt(1 * 64)) +
      p0
    );
  }

  readF32(): number {
    const value = this.#buffer.getFloat32(this.#offset, true);
    this.#offset += 4;
    return value;
  }

  readF64(): number {
    const value = this.#buffer.getFloat64(this.#offset, true);
    this.#offset += 8;
    return value;
  }

  readString(): string {
    const length = this.readU32();
    const uint8Array = new Uint8Array(
      this.#buffer.buffer,
      this.#offset,
      length
    );
    const decoder = new TextDecoder('utf-8');
    const value = decoder.decode(uint8Array);
    this.#offset += length;
    return value;
  }
}
