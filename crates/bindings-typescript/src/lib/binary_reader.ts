export default class BinaryReader {
  /**
   * The DataView used to read values from the binary data.
   *
   * Note: The DataView's `byteOffset` is relative to the beginning of the
   * underlying ArrayBuffer, not the start of the provided Uint8Array input.
   * This `BinaryReader`'s `#offset` field is used to track the current read position
   * relative to the start of the provided Uint8Array input.
   */
  view: DataView;

  /**
   * Represents the offset (in bytes) relative to the start of the DataView
   * and provided Uint8Array input.
   *
   * Note: This is *not* the absolute byte offset within the underlying ArrayBuffer.
   */
  offset: number = 0;

  constructor(input: Uint8Array | DataView) {
    this.view =
      input instanceof DataView
        ? input
        : new DataView(input.buffer, input.byteOffset, input.byteLength);
    this.offset = 0;
  }

  reset(input: Uint8Array | DataView) {
    this.view =
      input instanceof DataView
        ? input
        : new DataView(input.buffer, input.byteOffset, input.byteLength);
    this.offset = 0;
  }

  get remaining(): number {
    return this.view.byteLength - this.offset;
  }

  /** Ensure we have at least `n` bytes left to read */
  #ensure(n: number): void {
    if (this.offset + n > this.view.byteLength) {
      throw new RangeError(
        `Tried to read ${n} byte(s) at relative offset ${this.offset}, but only ${this.remaining} byte(s) remain`
      );
    }
  }

  readUInt8Array(): Uint8Array {
    const length = this.readU32();
    this.#ensure(length);
    // Return an owned copy, not a view over the reader's buffer. Decoded column
    // values are handed to user code and may be retained, but the runtime reuses
    // this buffer across table scans, so a view would be silently invalidated by
    // the next iter()/filter(). Callers that only consume the bytes transiently
    // (e.g. readString) can use readBytes directly to avoid this copy.
    // https://github.com/clockworklabs/SpacetimeDB/issues/5490
    return this.readBytes(length).slice();
  }

  readBool(): boolean {
    const value = this.view.getUint8(this.offset);
    this.offset += 1;
    return value !== 0;
  }

  readByte(): number {
    const value = this.view.getUint8(this.offset);
    this.offset += 1;
    return value;
  }

  readBytes(length: number): Uint8Array {
    // Returns a Uint8Array *view* over the reader's underlying buffer (no copy).
    // The view is only valid until the runtime reuses that buffer (e.g. the next
    // table scan). Consume it immediately or copy it (see readUInt8Array).
    //
    // The #view.buffer is the whole ArrayBuffer, so we need to account for the
    // #view's starting position in that buffer (#view.byteOffset) and the current #offset
    const array = new Uint8Array(
      this.view.buffer,
      this.view.byteOffset + this.offset,
      length
    );
    this.offset += length;
    return array;
  }

  readI8(): number {
    const value = this.view.getInt8(this.offset);
    this.offset += 1;
    return value;
  }

  readU8(): number {
    return this.readByte();
  }

  readI16(): number {
    const value = this.view.getInt16(this.offset, true);
    this.offset += 2;
    return value;
  }

  readU16(): number {
    const value = this.view.getUint16(this.offset, true);
    this.offset += 2;
    return value;
  }

  readI32(): number {
    const value = this.view.getInt32(this.offset, true);
    this.offset += 4;
    return value;
  }

  readU32(): number {
    const value = this.view.getUint32(this.offset, true);
    this.offset += 4;
    return value;
  }

  readI64(): bigint {
    const value = this.view.getBigInt64(this.offset, true);
    this.offset += 8;
    return value;
  }

  readU64(): bigint {
    const value = this.view.getBigUint64(this.offset, true);
    this.offset += 8;
    return value;
  }

  readU128(): bigint {
    const lowerPart = this.view.getBigUint64(this.offset, true);
    const upperPart = this.view.getBigUint64(this.offset + 8, true);
    this.offset += 16;

    return (upperPart << BigInt(64)) + lowerPart;
  }

  readI128(): bigint {
    const lowerPart = this.view.getBigUint64(this.offset, true);
    const upperPart = this.view.getBigInt64(this.offset + 8, true);
    this.offset += 16;

    return (upperPart << BigInt(64)) + lowerPart;
  }

  readU256(): bigint {
    const p0 = this.view.getBigUint64(this.offset, true);
    const p1 = this.view.getBigUint64(this.offset + 8, true);
    const p2 = this.view.getBigUint64(this.offset + 16, true);
    const p3 = this.view.getBigUint64(this.offset + 24, true);
    this.offset += 32;

    return (
      (p3 << BigInt(3 * 64)) +
      (p2 << BigInt(2 * 64)) +
      (p1 << BigInt(1 * 64)) +
      p0
    );
  }

  readI256(): bigint {
    const p0 = this.view.getBigUint64(this.offset, true);
    const p1 = this.view.getBigUint64(this.offset + 8, true);
    const p2 = this.view.getBigUint64(this.offset + 16, true);
    const p3 = this.view.getBigInt64(this.offset + 24, true);
    this.offset += 32;

    return (
      (p3 << BigInt(3 * 64)) +
      (p2 << BigInt(2 * 64)) +
      (p1 << BigInt(1 * 64)) +
      p0
    );
  }

  readF32(): number {
    const value = this.view.getFloat32(this.offset, true);
    this.offset += 4;
    return value;
  }

  readF64(): number {
    const value = this.view.getFloat64(this.offset, true);
    this.offset += 8;
    return value;
  }

  readString(): string {
    const length = this.readU32();
    this.#ensure(length);
    // A view is safe here: TextDecoder copies the bytes synchronously, so nothing
    // retains a reference to the reader's buffer. Avoids readUInt8Array's copy.
    const bytes = this.readBytes(length);
    return new TextDecoder('utf-8').decode(bytes);
  }
}
