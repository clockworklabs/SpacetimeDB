import { fromByteArray } from 'base64-js';

export class ResizableBuffer {
  #buffer: ArrayBuffer;

  constructor(init: number | ArrayBuffer) {
    this.#buffer = typeof init === 'number' ? new ArrayBuffer(init) : init;
  }

  get buffer(): ArrayBuffer {
    return this.#buffer;
  }

  get capacity(): number {
    return this.#buffer.byteLength;
  }

  grow(newSize: number, copy: boolean) {
    if (newSize <= this.#buffer.byteLength) return;
    if (copy) {
      this.#buffer = this.#buffer.transfer(newSize);
    } else {
      // invalidate the previous buffer
      this.#buffer.transfer();
      this.#buffer = new ArrayBuffer(newSize);
    }
  }

  transfer(): ArrayBuffer {
    return this.#buffer.transfer();
  }
}

export default class BinaryWriter {
  #buffer: ResizableBuffer;
  #view: DataView;
  #offset: number = 0;

  constructor(init: number | ResizableBuffer) {
    this.#buffer = typeof init === 'number' ? new ResizableBuffer(init) : init;
    this.#view = new DataView(this.#buffer.buffer);
  }

  #expandBuffer(additionalCapacity: number): void {
    const minCapacity = this.#offset + additionalCapacity + 1;
    if (minCapacity <= this.#buffer.capacity) return;
    let newCapacity = this.#buffer.capacity * 2;
    if (newCapacity < minCapacity) newCapacity = minCapacity;
    this.#buffer.grow(newCapacity, true);
    this.#view = new DataView(this.#buffer.buffer);
  }

  toBase64(): string {
    return fromByteArray(this.getBuffer());
  }

  getBuffer(): Uint8Array {
    return new Uint8Array(this.#buffer.buffer, 0, this.#offset);
  }

  get offset(): number {
    return this.#offset;
  }

  writeUInt8Array(value: Uint8Array): void {
    const length = value.length;

    this.#expandBuffer(4 + length);

    this.writeU32(length);
    new Uint8Array(this.#buffer.buffer, this.#offset).set(value);
    this.#offset += length;
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
    this.writeUInt8Array(encodedString);
  }
}
