"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
class BinaryWriter {
    buffer;
    view;
    offset = 0;
    constructor(size) {
        this.buffer = new Uint8Array(size);
        this.view = new DataView(this.buffer.buffer);
    }
    expandBuffer(additionalCapacity) {
        const minCapacity = this.offset + additionalCapacity + 1;
        if (minCapacity <= this.buffer.length)
            return;
        let newCapacity = this.buffer.length * 2;
        if (newCapacity < minCapacity)
            newCapacity = minCapacity;
        const newBuffer = new Uint8Array(newCapacity);
        newBuffer.set(this.buffer);
        this.buffer = newBuffer;
        this.view = new DataView(this.buffer.buffer);
    }
    getBuffer() {
        return this.buffer.slice(0, this.offset);
    }
    writeUInt8Array(value) {
        const length = value.length;
        this.expandBuffer(4 + length);
        this.writeU32(length);
        this.buffer.set(value, this.offset);
        this.offset += value.length;
    }
    writeBool(value) {
        this.expandBuffer(1);
        this.view.setUint8(this.offset, value ? 1 : 0);
        this.offset += 1;
    }
    writeByte(value) {
        this.expandBuffer(1);
        this.view.setUint8(this.offset, value);
        this.offset += 1;
    }
    writeI8(value) {
        this.expandBuffer(1);
        this.view.setInt8(this.offset, value);
        this.offset += 1;
    }
    writeU8(value) {
        this.expandBuffer(1);
        this.view.setUint8(this.offset, value);
        this.offset += 1;
    }
    writeI16(value) {
        this.expandBuffer(2);
        this.view.setInt16(this.offset, value, true);
        this.offset += 2;
    }
    writeU16(value) {
        this.expandBuffer(2);
        this.view.setUint16(this.offset, value, true);
        this.offset += 2;
    }
    writeI32(value) {
        this.expandBuffer(4);
        this.view.setInt32(this.offset, value, true);
        this.offset += 4;
    }
    writeU32(value) {
        this.expandBuffer(4);
        this.view.setUint32(this.offset, value, true);
        this.offset += 4;
    }
    writeI64(value) {
        this.expandBuffer(8);
        this.view.setBigInt64(this.offset, value, true);
        this.offset += 8;
    }
    writeU64(value) {
        this.expandBuffer(8);
        this.view.setBigUint64(this.offset, value, true);
        this.offset += 8;
    }
    writeU128(value) {
        this.expandBuffer(16);
        const lowerPart = value & BigInt("0xFFFFFFFFFFFFFFFF");
        const upperPart = value >> BigInt(64);
        this.view.setBigUint64(this.offset, lowerPart, true);
        this.view.setBigUint64(this.offset + 8, upperPart, true);
        this.offset += 16;
    }
    writeI128(value) {
        this.expandBuffer(16);
        const lowerPart = value & BigInt("0xFFFFFFFFFFFFFFFF");
        const upperPart = value >> BigInt(64);
        this.view.setBigInt64(this.offset, lowerPart, true);
        this.view.setBigInt64(this.offset + 8, upperPart, true);
        this.offset += 16;
    }
    writeF32(value) {
        this.expandBuffer(4);
        this.view.setFloat32(this.offset, value, true);
        this.offset += 4;
    }
    writeF64(value) {
        this.expandBuffer(8);
        this.view.setFloat64(this.offset, value, true);
        this.offset += 8;
    }
    writeString(value) {
        const encoder = new TextEncoder();
        const encodedString = encoder.encode(value);
        this.writeU32(encodedString.length);
        this.expandBuffer(encodedString.length);
        this.buffer.set(encodedString, this.offset);
        this.offset += encodedString.length;
    }
}
exports.default = BinaryWriter;
