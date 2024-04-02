export default class BinaryWriter {
    private buffer;
    private view;
    private offset;
    constructor(size: number);
    private expandBuffer;
    getBuffer(): Uint8Array;
    writeUInt8Array(value: Uint8Array): void;
    writeBool(value: boolean): void;
    writeByte(value: number): void;
    writeI8(value: number): void;
    writeU8(value: number): void;
    writeI16(value: number): void;
    writeU16(value: number): void;
    writeI32(value: number): void;
    writeU32(value: number): void;
    writeI64(value: bigint): void;
    writeU64(value: bigint): void;
    writeU128(value: bigint): void;
    writeI128(value: bigint): void;
    writeF32(value: number): void;
    writeF64(value: number): void;
    writeString(value: string): void;
}
