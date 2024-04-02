import { AlgebraicType, BuiltinType } from "./algebraic_type";
export interface Serializer {
    write(type: AlgebraicType, value: any): any;
    args(): any;
}
export declare class JSONSerializer {
    private content;
    private index;
    constructor();
    args(): any;
    serializeBuiltinType(type: BuiltinType, value: any): any;
    serializeType(type: AlgebraicType, value: any): any;
    write(type: AlgebraicType, value: any): void;
}
export declare class BinarySerializer {
    private writer;
    constructor();
    args(): any;
    getBuffer(): Uint8Array;
    write(type: AlgebraicType, value: any): void;
    writeBuiltinType(type: BuiltinType, value: any): void;
    writeByte(byte: number): void;
}
