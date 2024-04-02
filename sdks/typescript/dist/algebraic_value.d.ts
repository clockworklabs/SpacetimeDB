import { ProductType, SumType, AlgebraicType, BuiltinType } from "./algebraic_type";
import BinaryReader from "./binary_reader";
export interface ReducerArgsAdapter {
    next: () => ValueAdapter;
}
export declare class JSONReducerArgsAdapter {
    args: any[];
    index: number;
    constructor(args: any[]);
    next(): ValueAdapter;
}
export declare class BinaryReducerArgsAdapter {
    adapter: BinaryAdapter;
    constructor(adapter: BinaryAdapter);
    next(): ValueAdapter;
}
/** Defines the interface for deserialize `AlgebraicValue`s*/
export interface ValueAdapter {
    readUInt8Array: () => Uint8Array;
    readArray: (type: AlgebraicType) => AlgebraicValue[];
    readMap: (keyType: AlgebraicType, valueType: AlgebraicType) => Map<AlgebraicValue, AlgebraicValue>;
    readString: () => string;
    readSum: (type: SumType) => SumValue;
    readProduct: (type: ProductType) => ProductValue;
    readBool: () => boolean;
    readByte: () => number;
    readI8: () => number;
    readU8: () => number;
    readI16: () => number;
    readU16: () => number;
    readI32: () => number;
    readU32: () => number;
    readI64: () => BigInt;
    readU64: () => BigInt;
    readU128: () => BigInt;
    readI128: () => BigInt;
    readF32: () => number;
    readF64: () => number;
    callMethod<K extends keyof ValueAdapter>(methodName: K): any;
}
export declare class BinaryAdapter implements ValueAdapter {
    private reader;
    constructor(reader: BinaryReader);
    callMethod<K extends keyof ValueAdapter>(methodName: K): any;
    readUInt8Array(): Uint8Array;
    readArray(type: AlgebraicType): AlgebraicValue[];
    readMap(keyType: AlgebraicType, valueType: AlgebraicType): Map<AlgebraicValue, AlgebraicValue>;
    readString(): string;
    readSum(type: SumType): SumValue;
    readProduct(type: ProductType): ProductValue;
    readBool(): boolean;
    readByte(): number;
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
}
export declare class JSONAdapter implements ValueAdapter {
    private value;
    constructor(value: any);
    callMethod<K extends keyof ValueAdapter>(methodName: K): any;
    readUInt8Array(): Uint8Array;
    readArray(type: AlgebraicType): AlgebraicValue[];
    readMap(_keyType: AlgebraicType, _valueType: AlgebraicType): Map<AlgebraicValue, AlgebraicValue>;
    readString(): string;
    readSum(type: SumType): SumValue;
    readProduct(type: ProductType): ProductValue;
    readBool(): boolean;
    readByte(): number;
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
}
/** A value of a sum type choosing a specific variant of the type. */
export declare class SumValue {
    /** A tag representing the choice of one variant of the sum type's variants. */
    tag: number;
    /**
    * Given a variant `Var(Ty)` in a sum type `{ Var(Ty), ... }`,
    * this provides the `value` for `Ty`.
    */
    value: AlgebraicValue;
    constructor(tag: number, value: AlgebraicValue);
    static deserialize(type: SumType | undefined, adapter: ValueAdapter): SumValue;
}
/**
* A product value is made of a list of
* "elements" / "fields" / "factors" of other `AlgebraicValue`s.
*
* The type of product value is a [product type](`ProductType`).
*/
export declare class ProductValue {
    elements: AlgebraicValue[];
    constructor(elements: AlgebraicValue[]);
    static deserialize(type: ProductType | undefined, adapter: ValueAdapter): ProductValue;
}
/** A built-in value of a [`BuiltinType`]. */
type BuiltinValueType = boolean | string | number | AlgebraicValue[] | BigInt | Map<AlgebraicValue, AlgebraicValue> | Uint8Array;
export declare class BuiltinValue {
    value: BuiltinValueType;
    constructor(value: BuiltinValueType);
    static deserialize(type: BuiltinType, adapter: ValueAdapter): BuiltinValue;
    asString(): string;
    asArray(): AlgebraicValue[];
    asJsArray(type: string): any[];
    asNumber(): number;
    asBool(): boolean;
    asBigInt(): BigInt;
    asBoolean(): boolean;
    asBytes(): Uint8Array;
}
type AnyValue = SumValue | ProductValue | BuiltinValue;
/** A value in SATS. */
export declare class AlgebraicValue {
    /** A structural sum value. */
    sum: SumValue | undefined;
    /** A structural product value. */
    product: ProductValue | undefined;
    /** A builtin value that has a builtin type */
    builtin: BuiltinValue | undefined;
    constructor(value: AnyValue | undefined);
    callMethod<K extends keyof AlgebraicValue>(methodName: K): any;
    static deserialize(type: AlgebraicType, adapter: ValueAdapter): AlgebraicValue;
    asProductValue(): ProductValue;
    asBuiltinValue(): BuiltinValue;
    asSumValue(): SumValue;
    asArray(): AlgebraicValue[];
    asString(): string;
    asNumber(): number;
    asBool(): boolean;
    asBigInt(): BigInt;
    asBoolean(): boolean;
    asBytes(): Uint8Array;
    private assertBuiltin;
}
export {};
