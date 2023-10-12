import { ProductType, SumType, AlgebraicType, MapType } from "./algebraic_type";
import BinaryReader from "./binary_reader";

export interface ReducerArgsAdapter {
  next: () => ValueAdapter;
}

export class JSONReducerArgsAdapter {
  args: any[];
  index: number = 0;

  constructor(args: any[]) {
    this.args = args;
  }

  next(): ValueAdapter {
    if (this.index >= this.args.length) {
      throw "Number of arguments in the reducer is larger than what we got from the server";
    }

    const adapter = new JSONAdapter(this.args[this.index]);
    this.index += 1;
    return adapter;
  }
}

export class BinaryReducerArgsAdapter {
  adapter: BinaryAdapter;

  constructor(adapter: BinaryAdapter) {
    this.adapter = adapter;
  }

  next(): ValueAdapter {
    return this.adapter;
  }
}

/** Defines the interface for deserializing `AlgebraicValue`s*/
export interface ValueAdapter {
  readSum: (type: SumType) => SumValue;
  readProduct: (type: ProductType) => ProductValue;
  readUInt8Array: () => Uint8Array;
  readArray: (type: AlgebraicType) => AlgebraicValue[];
  readMap: (keyType: AlgebraicType, valueType: AlgebraicType) => MapValue;
  readBool: () => boolean;
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
  readString: () => string;
  readByte: () => number;

  callMethod<K extends keyof ValueAdapter>(methodName: K): any;
}

export class BinaryAdapter implements ValueAdapter {
  private reader: BinaryReader;

  constructor(reader: BinaryReader) {
    this.reader = reader;
  }

  callMethod<K extends keyof ValueAdapter>(methodName: K): any {
    return (this[methodName] as Function)();
  }

  readUInt8Array(): Uint8Array {
    const length = this.reader.readU32();
    return this.reader.readUInt8Array(length);
  }

  readArray(type: AlgebraicType): AlgebraicValue[] {
    const length = this.reader.readU32();
    let result: AlgebraicValue[] = [];
    for (let i = 0; i < length; i++) {
      result.push(AlgebraicValue.deserialize(type, this));
    }

    return result;
  }

  readMap(
    keyType: AlgebraicType,
    valueType: AlgebraicType
  ): Map<AlgebraicValue, AlgebraicValue> {
    const mapLength = this.reader.readU32();
    let result: Map<AlgebraicValue, AlgebraicValue> = new Map();
    for (let i = 0; i < mapLength; i++) {
      const key = AlgebraicValue.deserialize(keyType, this);
      const value = AlgebraicValue.deserialize(valueType, this);
      result.set(key, value);
    }

    return result;
  }

  readString(): string {
    const strLength = this.reader.readU32();
    return this.reader.readString(strLength);
  }

  readSum(type: SumType): SumValue {
    let tag = this.reader.readByte();
    let sumValue = AlgebraicValue.deserialize(
      type.variants[tag].algebraicType,
      this
    );
    return new SumValue(tag, sumValue);
  }

  readProduct(type: ProductType): ProductValue {
    let elements: AlgebraicValue[] = [];

    for (let element of type.elements) {
      elements.push(AlgebraicValue.deserialize(element.algebraicType, this));
    }
    return new ProductValue(elements);
  }

  readBool(): boolean {
    return this.reader.readBool();
  }
  readByte(): number {
    return this.reader.readByte();
  }
  readI8(): number {
    return this.reader.readI8();
  }
  readU8(): number {
    return this.reader.readU8();
  }
  readI16(): number {
    return this.reader.readI16();
  }
  readU16(): number {
    return this.reader.readU16();
  }
  readI32(): number {
    return this.reader.readI32();
  }
  readU32(): number {
    return this.reader.readU32();
  }
  readI64(): BigInt {
    return this.reader.readI64();
  }
  readU64(): BigInt {
    return this.reader.readU64();
  }
  readU128(): BigInt {
    return this.reader.readU128();
  }
  readI128(): BigInt {
    return this.reader.readI128();
  }
  readF32(): number {
    return this.reader.readF32();
  }
  readF64(): number {
    return this.reader.readF64();
  }
}

export class JSONAdapter implements ValueAdapter {
  private value: any;

  constructor(value: any) {
    this.value = value;
  }

  callMethod<K extends keyof ValueAdapter>(methodName: K): any {
    return (this[methodName] as Function)();
  }

  readUInt8Array(): Uint8Array {
    return Uint8Array.from(
      this.value.match(/.{1,2}/g).map((byte: string) => parseInt(byte, 16))
    );
  }

  readArray(type: AlgebraicType): AlgebraicValue[] {
    let result: AlgebraicValue[] = [];
    for (let el of this.value) {
      result.push(AlgebraicValue.deserialize(type, new JSONAdapter(el)));
    }

    return result;
  }

  readMap(
    _keyType: AlgebraicType,
    _valueType: AlgebraicType
  ): Map<AlgebraicValue, AlgebraicValue> {
    let result: Map<AlgebraicValue, AlgebraicValue> = new Map();
    // for (let i = 0; i < this.value.length; i++) {
    //   const key = AlgebraicValue.deserialize(
    //     keyType,
    //     new JSONAdapter()
    //   );
    //   const value = AlgebraicValue.deserialize(
    //     valueType,
    //     this
    //   );
    //   result.set(key, value);
    // }
    //
    return result;
  }

  readString(): string {
    return this.value;
  }

  readSum(type: SumType): SumValue {
    let tag = parseInt(Object.keys(this.value)[0]);
    let variant = type.variants[tag];
    let enumValue = Object.values(this.value)[0];
    let sumValue = AlgebraicValue.deserialize(
      variant.algebraicType,
      new JSONAdapter(enumValue)
    );
    return new SumValue(tag, sumValue);
  }

  readProduct(type: ProductType): ProductValue {
    let elements: AlgebraicValue[] = [];

    for (let i in type.elements) {
      let element = type.elements[i];
      elements.push(
        AlgebraicValue.deserialize(
          element.algebraicType,
          new JSONAdapter(this.value[i])
        )
      );
    }
    return new ProductValue(elements);
  }

  readBool(): boolean {
    return this.value;
  }
  readByte(): number {
    return this.value;
  }
  readI8(): number {
    return this.value;
  }
  readU8(): number {
    return this.value;
  }
  readI16(): number {
    return this.value;
  }
  readU16(): number {
    return this.value;
  }
  readI32(): number {
    return this.value;
  }
  readU32(): number {
    return this.value;
  }
  readI64(): BigInt {
    return this.value;
  }
  readU64(): BigInt {
    return this.value;
  }
  readU128(): BigInt {
    return this.value;
  }
  readI128(): BigInt {
    return this.value;
  }
  readF32(): number {
    return this.value;
  }
  readF64(): number {
    return this.value;
  }
}

/** A value of a sum type choosing a specific variant of the type. */
export class SumValue {
  /** A tag representing the choice of one variant of the sum type's variants. */
  public tag: number;
  /**
   * Given a variant `Var(Ty)` in a sum type `{ Var(Ty), ... }`,
   * this provides the `value` for `Ty`.
   */
  public value: AlgebraicValue;

  constructor(tag: number, value: AlgebraicValue) {
    this.tag = tag;
    this.value = value;
  }

  public static deserialize(
    type: SumType | undefined,
    adapter: ValueAdapter
  ): SumValue {
    if (type === undefined) {
      // TODO: get rid of undefined here
      throw "sum type is undefined";
    }

    return adapter.readSum(type);
  }
}

/**
 * A product value is made of a list of
 * "elements" / "fields" / "factors" of other `AlgebraicValue`s.
 *
 * The type of product value is a [product type](`ProductType`).
 */
export class ProductValue {
  elements: AlgebraicValue[];

  constructor(elements: AlgebraicValue[]) {
    this.elements = elements;
  }

  public static deserialize(
    type: ProductType | undefined,
    adapter: ValueAdapter
  ): ProductValue {
    if (type === undefined) {
      throw "type is undefined";
    }

    return adapter.readProduct(type);
  }
}

type MapValue = Map<AlgebraicValue, AlgebraicValue>;

type AnyValue =
  | SumValue
  | ProductValue
  | boolean
  | string
  | number
  | AlgebraicValue[]
  | BigInt
  | MapValue
  | Uint8Array;

/** A value in SATS. */
export class AlgebraicValue {
  value: AnyValue;

  constructor(value: AnyValue | undefined) {
    if (value === undefined) {
      // TODO: possibly get rid of it
      throw "value is undefined";
    }
    this.value = value;
  }

  callMethod<K extends keyof AlgebraicValue>(methodName: K): any {
    return (this[methodName] as Function)();
  }

  public static deserialize(type: AlgebraicType, adapter: ValueAdapter) {
    switch (type.type) {
      case AlgebraicType.Type.Sum:
        return new this(SumValue.deserialize(type.sum, adapter));
      case AlgebraicType.Type.Product:
        return new this(ProductValue.deserialize(type.product, adapter));
      case AlgebraicType.Type.Array:
        let elemType = type.array;
        if (elemType.type === AlgebraicType.Type.U8) {
          return new this(adapter.readUInt8Array());
        } else {
          return new this(adapter.readArray(elemType));
        }
      case AlgebraicType.Type.Map:
        let mapType = type.map;
        return new this(adapter.readMap(mapType.keyType, mapType.valueType));
      case AlgebraicType.Type.Bool:
        return new this(adapter.readBool());
      case AlgebraicType.Type.I8:
        return new this(adapter.readI8());
      case AlgebraicType.Type.U8:
        return new this(adapter.readU8());
      case AlgebraicType.Type.I16:
        return new this(adapter.readI16());
      case AlgebraicType.Type.U16:
        return new this(adapter.readU16());
      case AlgebraicType.Type.I32:
        return new this(adapter.readI32());
      case AlgebraicType.Type.U32:
        return new this(adapter.readU32());
      case AlgebraicType.Type.I64:
        return new this(adapter.readI64());
      case AlgebraicType.Type.U64:
        return new this(adapter.readU64());
      case AlgebraicType.Type.I128:
        return new this(adapter.readI128());
      case AlgebraicType.Type.U128:
        return new this(adapter.readU128());
      case AlgebraicType.Type.String:
        return new this(adapter.readString());
      default:
        throw new Error(`not implemented, ${type.type}`);
    }
  }

  public asSumValue(): SumValue {
    return this.value as SumValue;
  }

  public asProductValue(): ProductValue {
    return this.value as ProductValue;
  }

  public asArray(): AlgebraicValue[] {
    return this.value as AlgebraicValue[];
  }

  public asMap(): MapValue {
    return this.value as MapValue;
  }

  public asBool(): boolean {
    return this.value as boolean;
  }

  public asNumber(): number {
    return this.value as number;
  }

  public asBigInt(): BigInt {
    return this.value as BigInt;
  }

  public asString(): string {
    return this.value as string;
  }

  public asBytes(): Uint8Array {
    return this.value as Uint8Array;
  }

  public asJsArray(type: string): any[] {
    return this.asArray().map((el) =>
      el.callMethod(("as" + type) as keyof AlgebraicValue)
    );
  }
}
