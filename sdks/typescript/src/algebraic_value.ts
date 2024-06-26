import {
  ProductType,
  SumType,
  AlgebraicType,
  BuiltinType,
  // EnumLabel,
  MapType,
} from "./algebraic_type";
import BinaryReader from "./binary_reader";

export interface ReducerArgsAdapter {
  next: () => ValueAdapter;
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

/** Defines the interface for deserialize `AlgebraicValue`s*/
export interface ValueAdapter {
  readUInt8Array: () => Uint8Array;
  readArray: (type: AlgebraicType) => AlgebraicValue[];
  readMap: (
    keyType: AlgebraicType,
    valueType: AlgebraicType
  ) => Map<AlgebraicValue, AlgebraicValue>;
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
  readI64: () => bigint;
  readU64: () => bigint;
  readU128: () => bigint;
  readI128: () => bigint;
  readF32: () => number;
  readF64: () => number;

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
  readI64(): bigint {
    return this.reader.readI64();
  }
  readU64(): bigint {
    return this.reader.readU64();
  }
  readU128(): bigint {
    return this.reader.readU128();
  }
  readI128(): bigint {
    return this.reader.readI128();
  }
  readF32(): number {
    return this.reader.readF32();
  }
  readF64(): number {
    return this.reader.readF64();
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

/** A built-in value of a [`BuiltinType`]. */
type BuiltinValueType =
  | boolean
  | string
  | number
  | AlgebraicValue[]
  | bigint
  | Map<AlgebraicValue, AlgebraicValue>
  | Uint8Array;

export class BuiltinValue {
  value: BuiltinValueType;

  constructor(value: BuiltinValueType) {
    this.value = value;
  }

  public static deserialize(
    type: BuiltinType,
    adapter: ValueAdapter
  ): BuiltinValue {
    switch (type.type) {
      case BuiltinType.Type.Array:
        let arrayBuiltinType: BuiltinType.Type | undefined =
          type.arrayType &&
          type.arrayType.type === AlgebraicType.Type.BuiltinType
            ? type.arrayType.builtin.type
            : undefined;
        if (
          arrayBuiltinType !== undefined &&
          arrayBuiltinType === BuiltinType.Type.U8
        ) {
          const value = adapter.readUInt8Array();
          return new this(value);
        } else {
          const arrayResult = adapter.readArray(
            type.arrayType as AlgebraicType
          );
          return new this(arrayResult);
        }
      case BuiltinType.Type.Map:
        let keyType: AlgebraicType = (type.mapType as MapType).keyType;
        let valueType: AlgebraicType = (type.mapType as MapType).valueType;
        const mapResult = adapter.readMap(keyType, valueType);
        return new this(mapResult);
      case BuiltinType.Type.String:
        const result = adapter.readString();
        return new this(result);
      default:
        const methodName: string = "read" + type.type;
        return new this(adapter.callMethod(methodName as keyof ValueAdapter));
    }
  }

  public asString(): string {
    return this.value as string;
  }

  public asArray(): AlgebraicValue[] {
    return this.value as AlgebraicValue[];
  }

  public asJsArray(type: string): any[] {
    return this.asArray().map((el) =>
      el.callMethod(("as" + type) as keyof AlgebraicValue)
    );
  }

  public asNumber(): number {
    return this.value as number;
  }

  public asBool(): boolean {
    return this.value as boolean;
  }

  public asBigInt(): bigint {
    return this.value as bigint;
  }

  public asBoolean(): boolean {
    return this.value as boolean;
  }

  public asBytes(): Uint8Array {
    return this.value as Uint8Array;
  }
}

type AnyValue = SumValue | ProductValue | BuiltinValue;

/** A value in SATS. */
export class AlgebraicValue {
  /** A structural sum value. */
  sum: SumValue | undefined;
  /** A structural product value. */
  product: ProductValue | undefined;
  /** A builtin value that has a builtin type */
  builtin: BuiltinValue | undefined;

  constructor(value: AnyValue | undefined) {
    if (value === undefined) {
      // TODO: possibly get rid of it
      throw "value is undefined";
    }
    switch (value.constructor) {
      case SumValue:
        this.sum = value as SumValue;
        break;
      case ProductValue:
        this.product = value as ProductValue;
        break;
      case BuiltinValue:
        this.builtin = value as BuiltinValue;
        break;
    }
  }

  callMethod<K extends keyof AlgebraicValue>(methodName: K): any {
    return (this[methodName] as Function)();
  }

  public static deserialize(type: AlgebraicType, adapter: ValueAdapter) {
    switch (type.type) {
      case AlgebraicType.Type.ProductType:
        return new this(ProductValue.deserialize(type.product, adapter));
      case AlgebraicType.Type.SumType:
        return new this(SumValue.deserialize(type.sum, adapter));
      case AlgebraicType.Type.BuiltinType:
        return new this(BuiltinValue.deserialize(type.builtin, adapter));
      default:
        throw new Error("not implemented");
    }
  }

  public asProductValue(): ProductValue {
    if (!this.product) {
      throw "AlgebraicValue is not a ProductValue and product was requested";
    }
    return this.product as ProductValue;
  }

  public asBuiltinValue(): BuiltinValue {
    this.assertBuiltin();
    return this.builtin as BuiltinValue;
  }

  public asSumValue(): SumValue {
    if (!this.sum) {
      throw "AlgebraicValue is not a SumValue and a sum value was requested";
    }

    return this.sum as SumValue;
  }

  public asArray(): AlgebraicValue[] {
    this.assertBuiltin();
    return (this.builtin as BuiltinValue).asArray();
  }

  public asString(): string {
    this.assertBuiltin();
    return (this.builtin as BuiltinValue).asString();
  }

  public asNumber(): number {
    this.assertBuiltin();
    return (this.builtin as BuiltinValue).asNumber();
  }

  public asBool(): boolean {
    this.assertBuiltin();
    return (this.builtin as BuiltinValue).asBool();
  }

  public asBigInt(): bigint {
    this.assertBuiltin();
    return (this.builtin as BuiltinValue).asBigInt();
  }

  public asBoolean(): boolean {
    this.assertBuiltin();
    return (this.builtin as BuiltinValue).asBool();
  }

  public asBytes(): Uint8Array {
    this.assertBuiltin();
    return (this.builtin as BuiltinValue).asBytes();
  }

  private assertBuiltin() {
    if (!this.builtin) {
      throw "AlgebraicValue is not a BuiltinValue and a string was requested";
    }
  }
}

export interface ParseableType<ParsedType> {
  getAlgebraicType: () => AlgebraicType;
  fromValue: (value: AlgebraicValue) => ParsedType;
}

export function parseValue<ParsedType>(
  ty: ParseableType<ParsedType>,
  src: Uint8Array
): ParsedType {
  const algebraicType = ty.getAlgebraicType();
  const adapter = new BinaryAdapter(new BinaryReader(src));
  const algebraicValue = AlgebraicValue.deserialize(algebraicType, adapter);
  return ty.fromValue(algebraicValue);
}
