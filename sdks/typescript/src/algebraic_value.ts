import { Address } from "./address";
import {
  ProductType,
  SumType,
  AlgebraicType,
  // EnumLabel,
  MapType,
} from "./algebraic_type";
import BinaryReader from "./binary_reader";
import { Identity } from "./identity";

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
  readMap: (keyType: AlgebraicType, valueType: AlgebraicType) => MapValue;
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

  readMap(keyType: AlgebraicType, valueType: AlgebraicType): MapValue {
    const mapLength = this.reader.readU32();
    let result: MapValue = new Map();
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

export type MapValue = Map<AlgebraicValue, AlgebraicValue>;

type AnyValue =
  | SumValue
  | ProductValue
  | AlgebraicValue[]
  | Uint8Array
  | MapValue
  | string
  | boolean
  | number
  | bigint;

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
      case AlgebraicType.Type.ProductType:
        return new this(ProductValue.deserialize(type.product, adapter));
      case AlgebraicType.Type.SumType:
        return new this(SumValue.deserialize(type.sum, adapter));
      case AlgebraicType.Type.ArrayType:
        let elemType = type.array;
        if (elemType.type === AlgebraicType.Type.U8) {
          return new this(adapter.readUInt8Array());
        } else {
          return new this(adapter.readArray(elemType));
        }
      case AlgebraicType.Type.MapType:
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

  public asProductValue(): ProductValue {
    return this.value as ProductValue;
  }

  public asField(index: number): AlgebraicValue {
    return this.asProductValue().elements[index];
  }

  public asSumValue(): SumValue {
    return this.value as SumValue;
  }

  public asArray(): AlgebraicValue[] {
    return this.value as AlgebraicValue[];
  }

  public asMap(): MapValue {
    return this.value as MapValue;
  }

  public asString(): string {
    return this.value as string;
  }

  public asBoolean(): boolean {
    return this.value as boolean;
  }

  public asNumber(): number {
    return this.value as number;
  }

  public asBytes(): Uint8Array {
    return this.value as Uint8Array;
  }

  public asBigInt(): bigint {
    return this.value as bigint;
  }

  public asIdentity(): Identity {
    return new Identity(this.asField(0).asBytes());
  }

  public asAddress(): Address {
    return new Address(this.asField(0).asBytes());
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
