import { fromByteArray } from 'base64-js';

export default class BinaryWriter {
  #buffer: Uint8Array;
  #view: DataView;
  #offset: number = 0;

  constructor(size: number) {
    this.#buffer = new Uint8Array(size);
    this.#view = new DataView(this.#buffer.buffer);
  }

  #expandBuffer(additionalCapacity: number): void {
    const minCapacity = this.#offset + additionalCapacity + 1;
    if (minCapacity <= this.#buffer.length) return;
    let newCapacity = this.#buffer.length * 2;
    if (newCapacity < minCapacity) newCapacity = minCapacity;
    const newBuffer = new Uint8Array(newCapacity);
    newBuffer.set(this.#buffer);
    this.#buffer = newBuffer;
    this.#view = new DataView(this.#buffer.buffer);
  }

  toBase64(): string {
    return fromByteArray(this.#buffer.subarray(0, this.#offset));
  }

  getBuffer(): Uint8Array {
    return this.#buffer.slice(0, this.#offset);
  }

  get offset(): number {
    return this.#offset;
  }

  writeUInt8Array(value: Uint8Array): void {
    const length = value.length;

    this.#expandBuffer(4 + length);

    this.writeU32(length);
    this.#buffer.set(value, this.#offset);
    this.#offset += value.length;
  }

  writeBool(value: boolean): void {
    this.#expandBuffer(1);
    this.#view.setUint8(this.#offset, value ? 1 : 0);
    this.#offset += 1;
  }

  writeByte(value: number): void {
    this.#expandBuffer(1);
    this.#view.setUint8(this.#offset, value);
    this.#offset += 1;
  }

  writeI8(value: number): void {
    this.#expandBuffer(1);
    this.#view.setInt8(this.#offset, value);
    this.#offset += 1;
  }

  writeU8(value: number): void {
    this.#expandBuffer(1);
    this.#view.setUint8(this.#offset, value);
    this.#offset += 1;
  }

  writeI16(value: number): void {
    this.#expandBuffer(2);
    this.#view.setInt16(this.#offset, value, true);
    this.#offset += 2;
  }

  writeU16(value: number): void {
    this.#expandBuffer(2);
    this.#view.setUint16(this.#offset, value, true);
    this.#offset += 2;
  }

  writeI32(value: number): void {
    this.#expandBuffer(4);
    this.#view.setInt32(this.#offset, value, true);
    this.#offset += 4;
  }

  writeU32(value: number): void {
    this.#expandBuffer(4);
    this.#view.setUint32(this.#offset, value, true);
    this.#offset += 4;
  }

  writeI64(value: bigint): void {
    this.#expandBuffer(8);
    this.#view.setBigInt64(this.#offset, value, true);
    this.#offset += 8;
  }

  writeU64(value: bigint): void {
    this.#expandBuffer(8);
    this.#view.setBigUint64(this.#offset, value, true);
    this.#offset += 8;
  }

  writeU128(value: bigint): void {
    this.#expandBuffer(16);
    const lowerPart = value & BigInt('0xFFFFFFFFFFFFFFFF');
    const upperPart = value >> BigInt(64);
    this.#view.setBigUint64(this.#offset, lowerPart, true);
    this.#view.setBigUint64(this.#offset + 8, upperPart, true);
    this.#offset += 16;
  }

  writeI128(value: bigint): void {
    this.#expandBuffer(16);
    const lowerPart = value & BigInt('0xFFFFFFFFFFFFFFFF');
    const upperPart = value >> BigInt(64);
    this.#view.setBigInt64(this.#offset, lowerPart, true);
    this.#view.setBigInt64(this.#offset + 8, upperPart, true);
    this.#offset += 16;
  }

  writeU256(value: bigint): void {
    this.#expandBuffer(32);
    const low_64_mask = BigInt('0xFFFFFFFFFFFFFFFF');
    const p0 = value & low_64_mask;
    const p1 = (value >> BigInt(64 * 1)) & low_64_mask;
    const p2 = (value >> BigInt(64 * 2)) & low_64_mask;
    const p3 = value >> BigInt(64 * 3);
    this.#view.setBigUint64(this.#offset + 8 * 0, p0, true);
    this.#view.setBigUint64(this.#offset + 8 * 1, p1, true);
    this.#view.setBigUint64(this.#offset + 8 * 2, p2, true);
    this.#view.setBigUint64(this.#offset + 8 * 3, p3, true);
    this.#offset += 32;
  }

  writeI256(value: bigint): void {
    this.#expandBuffer(32);
    const low_64_mask = BigInt('0xFFFFFFFFFFFFFFFF');
    const p0 = value & low_64_mask;
    const p1 = (value >> BigInt(64 * 1)) & low_64_mask;
    const p2 = (value >> BigInt(64 * 2)) & low_64_mask;
    const p3 = value >> BigInt(64 * 3);
    this.#view.setBigUint64(this.#offset + 8 * 0, p0, true);
    this.#view.setBigUint64(this.#offset + 8 * 1, p1, true);
    this.#view.setBigUint64(this.#offset + 8 * 2, p2, true);
    this.#view.setBigInt64(this.#offset + 8 * 3, p3, true);
    this.#offset += 32;
  }

  writeF32(value: number): void {
    this.#expandBuffer(4);
    this.#view.setFloat32(this.#offset, value, true);
    this.#offset += 4;
  }

  writeF64(value: number): void {
    this.#expandBuffer(8);
    this.#view.setFloat64(this.#offset, value, true);
    this.#offset += 8;
  }

  writeString(value: string): void {
    const encoder = new TextEncoder();
    const encodedString = encoder.encode(value);
    this.writeU32(encodedString.length);
    this.#expandBuffer(encodedString.length);
    this.#buffer.set(encodedString, this.#offset);
    this.#offset += encodedString.length;
  }
}
