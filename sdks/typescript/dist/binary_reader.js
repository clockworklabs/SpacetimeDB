"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
class BinaryReader {
    buffer;
    offset = 0;
    constructor(input) {
        this.buffer = new DataView(input.buffer);
        this.offset = input.byteOffset;
    }
    readUInt8Array(length) {
        const value = new Uint8Array(this.buffer.buffer, this.offset, length);
        this.offset += length;
        return value;
    }
    readBool() {
        const value = this.buffer.getUint8(this.offset);
        this.offset += 1;
        return value !== 0;
    }
    readByte() {
        const value = this.buffer.getUint8(this.offset);
        this.offset += 1;
        return value;
    }
    readBytes(length) {
        const value = new DataView(this.buffer.buffer, this.offset, length);
        this.offset += length;
        return new Uint8Array(value.buffer);
    }
    readI8() {
        const value = this.buffer.getInt8(this.offset);
        this.offset += 1;
        return value;
    }
    readU8() {
        const value = this.buffer.getUint8(this.offset);
        this.offset += 1;
        return value;
    }
    readI16() {
        const value = this.buffer.getInt16(this.offset, true);
        this.offset += 2;
        return value;
    }
    readU16() {
        const value = this.buffer.getUint16(this.offset, true);
        this.offset += 2;
        return value;
    }
    readI32() {
        const value = this.buffer.getInt32(this.offset, true);
        this.offset += 4;
        return value;
    }
    readU32() {
        const value = this.buffer.getUint32(this.offset, true);
        this.offset += 4;
        return value;
    }
    readI64() {
        const value = this.buffer.getBigInt64(this.offset, true);
        this.offset += 8;
        return value;
    }
    readU64() {
        const value = this.buffer.getBigUint64(this.offset, true);
        this.offset += 8;
        return value;
    }
    readU128() {
        const lowerPart = this.buffer.getBigUint64(this.offset, true);
        const upperPart = this.buffer.getBigUint64(this.offset + 8, true);
        this.offset += 16;
        return (upperPart << BigInt(64)) + lowerPart;
    }
    readI128() {
        const lowerPart = this.buffer.getBigInt64(this.offset, true);
        const upperPart = this.buffer.getBigInt64(this.offset + 8, true);
        this.offset += 16;
        return (upperPart << BigInt(64)) + lowerPart;
    }
    readF32() {
        const value = this.buffer.getFloat32(this.offset, true);
        this.offset += 4;
        return value;
    }
    readF64() {
        const value = this.buffer.getFloat64(this.offset, true);
        this.offset += 8;
        return value;
    }
    readString(length) {
        const uint8Array = new Uint8Array(this.buffer.buffer, this.offset, length);
        const decoder = new TextDecoder("utf-8");
        const value = decoder.decode(uint8Array);
        this.offset += length;
        return value;
    }
}
exports.default = BinaryReader;
