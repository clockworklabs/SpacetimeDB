export default class BinaryReader {
    private buffer;
    private offset;
    constructor(input: Uint8Array);
    readUInt8Array(length: number): Uint8Array;
    readBool(): boolean;
    readByte(): number;
    readBytes(length: number): Uint8Array;
    readI8(): number;
    readU8(): number;
    readI16(): number;
    readU16(): number;
    readI32(): number;
    readU32(): number;
    readI64(): BigInt;
    readU64(): BigInt;
    readU128(): BigInt;
    readI128(): BigInt;
    readF32(): number;
    readF64(): number;
    readString(length: number): string;
}
