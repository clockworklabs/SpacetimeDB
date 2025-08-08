import { TimeDuration } from './time_duration';
import { Timestamp } from './timestamp';
import { ConnectionId } from './connection_id';
import type BinaryReader from './binary_reader';
import BinaryWriter from './binary_writer';
import { Identity } from './identity';
import ScheduleAt from './schedule_at';

/**
 * A variant of a sum type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export class SumTypeVariant {
  name: string;
  algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

/**
 * Unlike most languages, sums in SATS are *[structural]* and not nominal.
 * When checking whether two nominal types are the same,
 * their names and/or declaration sites (e.g., module / namespace) are considered.
 * Meanwhile, a structural type system would only check the structure of the type itself,
 * e.g., the names of its variants and their inner data types in the case of a sum.
 *
 * This is also known as a discriminated union (implementation) or disjoint union.
 * Another name is [coproduct (category theory)](https://ncatlab.org/nlab/show/coproduct).
 *
 * These structures are known as sum types because the number of possible values a sum
 * ```ignore
 * { N_0(T_0), N_1(T_1), ..., N_n(T_n) }
 * ```
 * is:
 * ```ignore
 * Σ (i ∈ 0..n). values(T_i)
 * ```
 * so for example, `values({ A(U64), B(Bool) }) = values(U64) + values(Bool)`.
 *
 * See also: https://ncatlab.org/nlab/show/sum+type.
 *
 * [structural]: https://en.wikipedia.org/wiki/Structural_type_system
 */
export class SumType {
  variants: SumTypeVariant[];

  constructor(variants: SumTypeVariant[]) {
    this.variants = variants;
  }

  serialize = (writer: BinaryWriter, value: any): void => {
    // In TypeScript we handle Option values as a special case
    // we don't represent the some and none variants, but instead
    // we represent the value directly.
    if (
      this.variants.length == 2 &&
      this.variants[0].name === 'some' &&
      this.variants[1].name === 'none'
    ) {
      if (value !== null && value !== undefined) {
        writer.writeByte(0);
        this.variants[0].algebraicType.serialize(writer, value);
      } else {
        writer.writeByte(1);
      }
    } else {
      let variant = value['tag'];
      const index = this.variants.findIndex(v => v.name === variant);
      if (index < 0) {
        throw `Can't serialize a sum type, couldn't find ${value.tag} tag`;
      }
      writer.writeU8(index);
      this.variants[index].algebraicType.serialize(writer, value['value']);
    }
  };

  deserialize = (reader: BinaryReader): any => {
    let tag = reader.readU8();
    // In TypeScript we handle Option values as a special case
    // we don't represent the some and none variants, but instead
    // we represent the value directly.
    if (
      this.variants.length == 2 &&
      this.variants[0].name === 'some' &&
      this.variants[1].name === 'none'
    ) {
      if (tag === 0) {
        return this.variants[0].algebraicType.deserialize(reader);
      } else if (tag === 1) {
        return undefined;
      } else {
        throw `Can't deserialize an option type, couldn't find ${tag} tag`;
      }
    } else {
      let variant = this.variants[tag];
      let value = variant.algebraicType.deserialize(reader);
      return { tag: variant.name, value };
    }
  };
}

/**
 * A factor / element of a product type.
 *
 * An element consist of an optional name and a type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export class ProductTypeElement {
  name: string;
  algebraicType: AlgebraicType;

  constructor(name: string, algebraicType: AlgebraicType) {
    this.name = name;
    this.algebraicType = algebraicType;
  }
}

/**
 * A structural product type  of the factors given by `elements`.
 *
 * This is also known as `struct` and `tuple` in many languages,
 * but note that unlike most languages, products in SATs are *[structural]* and not nominal.
 * When checking whether two nominal types are the same,
 * their names and/or declaration sites (e.g., module / namespace) are considered.
 * Meanwhile, a structural type system would only check the structure of the type itself,
 * e.g., the names of its fields and their types in the case of a record.
 * The name "product" comes from category theory.
 *
 * See also: https://ncatlab.org/nlab/show/product+type.
 *
 * These structures are known as product types because the number of possible values in product
 * ```ignore
 * { N_0: T_0, N_1: T_1, ..., N_n: T_n }
 * ```
 * is:
 * ```ignore
 * Π (i ∈ 0..n). values(T_i)
 * ```
 * so for example, `values({ A: U64, B: Bool }) = values(U64) * values(Bool)`.
 *
 * [structural]: https://en.wikipedia.org/wiki/Structural_type_system
 */
export class ProductType {
  elements: ProductTypeElement[];

  constructor(elements: ProductTypeElement[]) {
    this.elements = elements;
  }

  isEmpty(): boolean {
    return this.elements.length === 0;
  }

  serialize = (writer: BinaryWriter, value: object): void => {
    for (let element of this.elements) {
      element.algebraicType.serialize(writer, value[element.name]);
    }
  };

  intoMapKey(value: any): ComparablePrimitive {
    if (this.elements.length === 1) {
      if (this.elements[0].name === '__time_duration_micros__') {
        return (value as TimeDuration).__time_duration_micros__;
      }

      if (this.elements[0].name === '__timestamp_micros_since_unix_epoch__') {
        return (value as Timestamp).__timestamp_micros_since_unix_epoch__;
      }

      if (this.elements[0].name === '__identity__') {
        return (value as Identity).__identity__;
      }

      if (this.elements[0].name === '__connection_id__') {
        return (value as ConnectionId).__connection_id__;
      }
    }
    // The fallback is to serialize and base64 encode the bytes.
    const writer = new BinaryWriter(10);
    this.serialize(writer, value);
    return writer.toBase64();
  }

  deserialize = (reader: BinaryReader): { [key: string]: any } => {
    let result: { [key: string]: any } = {};
    if (this.elements.length === 1) {
      if (this.elements[0].name === '__time_duration_micros__') {
        return new TimeDuration(reader.readI64());
      }

      if (this.elements[0].name === '__timestamp_micros_since_unix_epoch__') {
        return new Timestamp(reader.readI64());
      }

      if (this.elements[0].name === '__identity__') {
        return new Identity(reader.readU256());
      }

      if (this.elements[0].name === '__connection_id__') {
        return new ConnectionId(reader.readU128());
      }
    }

    for (let element of this.elements) {
      result[element.name] = element.algebraicType.deserialize(reader);
    }
    return result;
  };
}

/* A map type from keys of type `keyType` to values of type `valueType`. */
export class MapType {
  keyType: AlgebraicType;
  valueType: AlgebraicType;

  constructor(keyType: AlgebraicType, valueType: AlgebraicType) {
    this.keyType = keyType;
    this.valueType = valueType;
  }
}

type ArrayBaseType = AlgebraicType;
type TypeRef = null;
type None = null;
export type EnumLabel = { label: string };

type AnyType =
  | ProductType
  | SumType
  | ArrayBaseType
  | MapType
  | EnumLabel
  | TypeRef
  | None;

export type ComparablePrimitive = number | string | String | boolean | bigint;

/**
 * The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
 * which a nominal type system can be constructed.
 *
 * The type system unifies the concepts sum types, product types, and built-in
 * primitive types into a single type system.
 */
export class AlgebraicType {
  type!: Type;
  type_?: AnyType;

  #setter(type: Type, payload: AnyType | undefined) {
    this.type_ = payload;
    this.type = payload === undefined ? Type.None : type;
  }

  get product(): ProductType {
    if (this.type !== Type.ProductType) {
      throw 'product type was requested, but the type is not ProductType';
    }
    return this.type_ as ProductType;
  }

  set product(value: ProductType | undefined) {
    this.#setter(Type.ProductType, value);
  }

  get sum(): SumType {
    if (this.type !== Type.SumType) {
      throw 'sum type was requested, but the type is not SumType';
    }
    return this.type_ as SumType;
  }
  set sum(value: SumType | undefined) {
    this.#setter(Type.SumType, value);
  }

  get array(): ArrayBaseType {
    if (this.type !== Type.ArrayType) {
      throw 'array type was requested, but the type is not ArrayType';
    }
    return this.type_ as ArrayBaseType;
  }
  set array(value: ArrayBaseType | undefined) {
    this.#setter(Type.ArrayType, value);
  }

  get map(): MapType {
    if (this.type !== Type.MapType) {
      throw 'map type was requested, but the type is not MapType';
    }
    return this.type_ as MapType;
  }
  set map(value: MapType | undefined) {
    this.#setter(Type.MapType, value);
  }

  static #createType(type: Type, payload: AnyType | undefined): AlgebraicType {
    let at = new AlgebraicType();
    at.#setter(type, payload);
    return at;
  }

  static createProductType(elements: ProductTypeElement[]): AlgebraicType {
    return this.#createType(Type.ProductType, new ProductType(elements));
  }

  static createSumType(variants: SumTypeVariant[]): AlgebraicType {
    return this.#createType(Type.SumType, new SumType(variants));
  }

  static createArrayType(elementType: AlgebraicType): AlgebraicType {
    return this.#createType(Type.ArrayType, elementType);
  }

  static createMapType(key: AlgebraicType, val: AlgebraicType): AlgebraicType {
    return this.#createType(Type.MapType, new MapType(key, val));
  }

  static createBoolType(): AlgebraicType {
    return this.#createType(Type.Bool, null);
  }
  static createI8Type(): AlgebraicType {
    return this.#createType(Type.I8, null);
  }
  static createU8Type(): AlgebraicType {
    return this.#createType(Type.U8, null);
  }
  static createI16Type(): AlgebraicType {
    return this.#createType(Type.I16, null);
  }
  static createU16Type(): AlgebraicType {
    return this.#createType(Type.U16, null);
  }
  static createI32Type(): AlgebraicType {
    return this.#createType(Type.I32, null);
  }
  static createU32Type(): AlgebraicType {
    return this.#createType(Type.U32, null);
  }
  static createI64Type(): AlgebraicType {
    return this.#createType(Type.I64, null);
  }
  static createU64Type(): AlgebraicType {
    return this.#createType(Type.U64, null);
  }
  static createI128Type(): AlgebraicType {
    return this.#createType(Type.I128, null);
  }
  static createU128Type(): AlgebraicType {
    return this.#createType(Type.U128, null);
  }
  static createI256Type(): AlgebraicType {
    return this.#createType(Type.I256, null);
  }
  static createU256Type(): AlgebraicType {
    return this.#createType(Type.U256, null);
  }
  static createF32Type(): AlgebraicType {
    return this.#createType(Type.F32, null);
  }
  static createF64Type(): AlgebraicType {
    return this.#createType(Type.F64, null);
  }
  static createStringType(): AlgebraicType {
    return this.#createType(Type.String, null);
  }
  static createBytesType(): AlgebraicType {
    return this.createArrayType(this.createU8Type());
  }
  static createOptionType(innerType: AlgebraicType): AlgebraicType {
    return this.createSumType([
      new SumTypeVariant('some', innerType),
      new SumTypeVariant('none', this.createProductType([])),
    ]);
  }
  static createIdentityType(): AlgebraicType {
    return this.createProductType([
      new ProductTypeElement('__identity__', this.createU256Type()),
    ]);
  }

  static createConnectionIdType(): AlgebraicType {
    return this.createProductType([
      new ProductTypeElement('__connection_id__', this.createU128Type()),
    ]);
  }

  static createScheduleAtType(): AlgebraicType {
    return ScheduleAt.getAlgebraicType();
  }

  static createTimestampType(): AlgebraicType {
    return this.createProductType([
      new ProductTypeElement(
        '__timestamp_micros_since_unix_epoch__',
        this.createI64Type()
      ),
    ]);
  }

  static createTimeDurationType(): AlgebraicType {
    return this.createProductType([
      new ProductTypeElement('__time_duration_micros__', this.createI64Type()),
    ]);
  }

  isProductType(): boolean {
    return this.type === Type.ProductType;
  }

  isSumType(): boolean {
    return this.type === Type.SumType;
  }

  isArrayType(): boolean {
    return this.type === Type.ArrayType;
  }

  isMapType(): boolean {
    return this.type === Type.MapType;
  }

  #isBytes(): boolean {
    return this.isArrayType() && this.array.type == Type.U8;
  }

  #isBytesNewtype(tag: string): boolean {
    return (
      this.isProductType() &&
      this.product.elements.length === 1 &&
      (this.product.elements[0].algebraicType.type == Type.U128 ||
        this.product.elements[0].algebraicType.type == Type.U256) &&
      this.product.elements[0].name === tag
    );
  }

  #isI64Newtype(tag: string): boolean {
    return (
      this.isProductType() &&
      this.product.elements.length === 1 &&
      this.product.elements[0].algebraicType.type === Type.I64 &&
      this.product.elements[0].name === tag
    );
  }

  isIdentity(): boolean {
    return this.#isBytesNewtype('__identity__');
  }

  isConnectionId(): boolean {
    return this.#isBytesNewtype('__connection_id__');
  }

  isScheduleAt(): boolean {
    return (
      this.isSumType() &&
      this.sum.variants.length === 2 &&
      this.sum.variants[0].name === 'Interval' &&
      this.sum.variants[0].algebraicType.type === Type.U64 &&
      this.sum.variants[1].name === 'Time' &&
      this.sum.variants[1].algebraicType.type === Type.U64
    );
  }

  isTimestamp(): boolean {
    return this.#isI64Newtype('__timestamp_micros_since_unix_epoch__');
  }

  isTimeDuration(): boolean {
    return this.#isI64Newtype('__time_duration_micros__');
  }

  /**
   * Convert a value of the algebraic type into something that can be used as a key in a map.
   * There are no guarantees about being able to order it.
   * This is only guaranteed to be comparable to other values of the same type.
   * @param value A value of the algebraic type
   * @returns Something that can be used as a key in a map.
   */
  intoMapKey(value: any): ComparablePrimitive {
    switch (this.type) {
      case Type.U8:
      case Type.U16:
      case Type.U32:
      case Type.U64:
      case Type.U128:
      case Type.U256:
      case Type.I8:
      case Type.I16:
      case Type.I64:
      case Type.I128:
      case Type.F32:
      case Type.F64:
      case Type.String:
      case Type.Bool:
        return value;
      case Type.ProductType:
        return this.product.intoMapKey(value);
      default:
        const writer = new BinaryWriter(10);
        this.serialize(writer, value);
        return writer.toBase64();
    }
  }

  serialize(writer: BinaryWriter, value: any): void {
    switch (this.type) {
      case Type.ProductType:
        this.product.serialize(writer, value);
        break;
      case Type.SumType:
        this.sum.serialize(writer, value);
        break;
      case Type.ArrayType:
        if (this.#isBytes()) {
          writer.writeUInt8Array(value);
        } else {
          const elemType = this.array;
          writer.writeU32(value.length);
          for (let elem of value) {
            elemType.serialize(writer, elem);
          }
        }
        break;
      case Type.MapType:
        throw new Error('not implemented');
      case Type.Bool:
        writer.writeBool(value);
        break;
      case Type.I8:
        writer.writeI8(value);
        break;
      case Type.U8:
        writer.writeU8(value);
        break;
      case Type.I16:
        writer.writeI16(value);
        break;
      case Type.U16:
        writer.writeU16(value);
        break;
      case Type.I32:
        writer.writeI32(value);
        break;
      case Type.U32:
        writer.writeU32(value);
        break;
      case Type.I64:
        writer.writeI64(value);
        break;
      case Type.U64:
        writer.writeU64(value);
        break;
      case Type.I128:
        writer.writeI128(value);
        break;
      case Type.U128:
        writer.writeU128(value);
        break;
      case Type.I256:
        writer.writeI256(value);
        break;
      case Type.U256:
        writer.writeU256(value);
        break;
      case Type.F32:
        writer.writeF32(value);
        break;
      case Type.F64:
        writer.writeF64(value);
        break;
      case Type.String:
        writer.writeString(value);
        break;
      default:
        throw new Error(`not implemented, ${this.type}`);
    }
  }

  deserialize(reader: BinaryReader): any {
    switch (this.type) {
      case Type.ProductType:
        return this.product.deserialize(reader);
      case Type.SumType:
        return this.sum.deserialize(reader);
      case Type.ArrayType:
        if (this.#isBytes()) {
          return reader.readUInt8Array();
        } else {
          const elemType = this.array;
          const length = reader.readU32();
          let result: any[] = [];
          for (let i = 0; i < length; i++) {
            result.push(elemType.deserialize(reader));
          }
          return result;
        }
      case Type.MapType:
        // TODO: MapType is being removed
        throw new Error('not implemented');
      case Type.Bool:
        return reader.readBool();
      case Type.I8:
        return reader.readI8();
      case Type.U8:
        return reader.readU8();
      case Type.I16:
        return reader.readI16();
      case Type.U16:
        return reader.readU16();
      case Type.I32:
        return reader.readI32();
      case Type.U32:
        return reader.readU32();
      case Type.I64:
        return reader.readI64();
      case Type.U64:
        return reader.readU64();
      case Type.I128:
        return reader.readI128();
      case Type.U128:
        return reader.readU128();
      case Type.U256:
        return reader.readU256();
      case Type.F32:
        return reader.readF32();
      case Type.F64:
        return reader.readF64();
      case Type.String:
        return reader.readString();
      default:
        throw new Error(`not implemented, ${this.type}`);
    }
  }
}

export namespace AlgebraicType {
  export enum Type {
    SumType = 'SumType',
    ProductType = 'ProductType',
    ArrayType = 'ArrayType',
    MapType = 'MapType',
    Bool = 'Bool',
    I8 = 'I8',
    U8 = 'U8',
    I16 = 'I16',
    U16 = 'U16',
    I32 = 'I32',
    U32 = 'U32',
    I64 = 'I64',
    U64 = 'U64',
    I128 = 'I128',
    U128 = 'U128',
    I256 = 'I256',
    U256 = 'U256',
    F32 = 'F32',
    F64 = 'F64',
    /** UTF-8 encoded */
    String = 'String',
    None = 'None',
  }
}

// No idea why but in order to have a local alias for both of these
// need to be present
type Type = AlgebraicType.Type;
let Type: typeof AlgebraicType.Type = AlgebraicType.Type;
