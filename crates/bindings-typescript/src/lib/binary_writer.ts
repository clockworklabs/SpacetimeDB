import { fromByteArray } from 'base64-js';

const ArrayBufferPrototypeTransfer =
  ArrayBuffer.prototype.transfer ??
  function (this: ArrayBuffer, newByteLength) {
    if (newByteLength === undefined) {
      return this.slice();
    } else if (newByteLength <= this.byteLength) {
      return this.slice(0, newByteLength);
    } else {
      const copy = new Uint8Array(newByteLength);
      copy.set(new Uint8Array(this));
      return copy.buffer;
    }
  };

export default class BinaryWriter {
  #buffer: ArrayBuffer;
  view: DataView;
  offset: number = 0;

  constructor(size: number) {
    this.#buffer = new ArrayBuffer(size);
    this.view = new DataView(this.#buffer);
  }

  expandBuffer(additionalCapacity: number): void {
    const minCapacity = this.offset + additionalCapacity + 1;
    if (minCapacity <= this.#buffer.byteLength) return;
    let newCapacity = this.#buffer.byteLength * 2;
    if (newCapacity < minCapacity) newCapacity = minCapacity;
    this.#buffer = ArrayBufferPrototypeTransfer.call(this.#buffer, newCapacity);
    this.view = new DataView(this.#buffer);
  }

  toBase64(): string {
    return fromByteArray(this.getBuffer());
  }

  getBuffer(): Uint8Array {
    return new Uint8Array(this.#buffer, 0, this.offset);
  }

  writeUInt8Array(value: Uint8Array): void {
    const length = value.length;

    this.expandBuffer(4 + length);

    this.writeU32(length);
    new Uint8Array(this.#buffer, this.offset).set(value);
    this.offset += length;
  }

  writeBool(value: boolean): void {
    this.expandBuffer(1);
    this.view.setUint8(this.offset, value ? 1 : 0);
    this.offset += 1;
  }

  writeByte(value: number): void {
    this.expandBuffer(1);
    this.view.setUint8(this.offset, value);
    this.offset += 1;
  }

  writeI8(value: number): void {
    this.expandBuffer(1);
    this.view.setInt8(this.offset, value);
    this.offset += 1;
  }

  writeU8(value: number): void {
    this.expandBuffer(1);
    this.view.setUint8(this.offset, value);
    this.offset += 1;
  }

  writeI16(value: number): void {
    this.expandBuffer(2);
    this.view.setInt16(this.offset, value, true);
    this.offset += 2;
  }

  writeU16(value: number): void {
    this.expandBuffer(2);
    this.view.setUint16(this.offset, value, true);
    this.offset += 2;
  }

  writeI32(value: number): void {
    this.expandBuffer(4);
    this.view.setInt32(this.offset, value, true);
    this.offset += 4;
  }

  writeU32(value: number): void {
    this.expandBuffer(4);
    this.view.setUint32(this.offset, value, true);
    this.offset += 4;
  }

  writeI64(value: bigint): void {
    this.expandBuffer(8);
    this.view.setBigInt64(this.offset, value, true);
    this.offset += 8;
  }

  writeU64(value: bigint): void {
    this.expandBuffer(8);
    this.view.setBigUint64(this.offset, value, true);
    this.offset += 8;
  }

  writeU128(value: bigint): void {
    this.expandBuffer(16);
    const lowerPart = value & BigInt('0xFFFFFFFFFFFFFFFF');
    const upperPart = value >> BigInt(64);
    this.view.setBigUint64(this.offset, lowerPart, true);
    this.view.setBigUint64(this.offset + 8, upperPart, true);
    this.offset += 16;
  }

  writeI128(value: bigint): void {
    this.expandBuffer(16);
    const lowerPart = value & BigInt('0xFFFFFFFFFFFFFFFF');
    const upperPart = value >> BigInt(64);
    this.view.setBigInt64(this.offset, lowerPart, true);
    this.view.setBigInt64(this.offset + 8, upperPart, true);
    this.offset += 16;
  }

  writeU256(value: bigint): void {
    this.expandBuffer(32);
    const low_64_mask = BigInt('0xFFFFFFFFFFFFFFFF');
    const p0 = value & low_64_mask;
    const p1 = (value >> BigInt(64 * 1)) & low_64_mask;
    const p2 = (value >> BigInt(64 * 2)) & low_64_mask;
    const p3 = value >> BigInt(64 * 3);
    this.view.setBigUint64(this.offset + 8 * 0, p0, true);
    this.view.setBigUint64(this.offset + 8 * 1, p1, true);
    this.view.setBigUint64(this.offset + 8 * 2, p2, true);
    this.view.setBigUint64(this.offset + 8 * 3, p3, true);
    this.offset += 32;
  }

  writeI256(value: bigint): void {
    this.expandBuffer(32);
    const low_64_mask = BigInt('0xFFFFFFFFFFFFFFFF');
    const p0 = value & low_64_mask;
    const p1 = (value >> BigInt(64 * 1)) & low_64_mask;
    const p2 = (value >> BigInt(64 * 2)) & low_64_mask;
    const p3 = value >> BigInt(64 * 3);
    this.view.setBigUint64(this.offset + 8 * 0, p0, true);
    this.view.setBigUint64(this.offset + 8 * 1, p1, true);
    this.view.setBigUint64(this.offset + 8 * 2, p2, true);
    this.view.setBigInt64(this.offset + 8 * 3, p3, true);
    this.offset += 32;
  }

  writeF32(value: number): void {
    this.expandBuffer(4);
    this.view.setFloat32(this.offset, value, true);
    this.offset += 4;
  }

  writeF64(value: number): void {
    this.expandBuffer(8);
    this.view.setFloat64(this.offset, value, true);
    this.offset += 8;
  }

  writeString(value: string): void {
    const encoder = new TextEncoder();
    const encodedString = encoder.encode(value);
    this.writeUInt8Array(encodedString);
  }
}
