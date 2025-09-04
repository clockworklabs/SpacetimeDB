import { TimeDuration } from './time_duration';
import { Timestamp } from './timestamp';
import { ConnectionId } from './connection_id';
import type BinaryReader from './binary_reader';
import BinaryWriter from './binary_writer';
import { Identity } from './identity';
import {
  AlgebraicType as AlgebraicTypeType,
  AlgebraicType as AlgebraicTypeValue,
} from './autogen/algebraic_type_type';
import {
  type ProductType as ProductTypeType,
  ProductType as ProductTypeValue,
} from './autogen/product_type_type';
import {
  type SumType as SumTypeType,
  SumType as SumTypeValue,
} from './autogen/sum_type_type';
import ScheduleAt from './schedule_at';

/**
 * A factor / element of a product type.
 *
 * An element consist of an optional name and a type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export * from './autogen/product_type_element_type';

/**
 * A variant of a sum type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export * from './autogen/sum_type_variant_type';

/**
 * The variant types of the Algebraic Type tagged union.
 */
export type * as AlgebraicTypeVariants from './autogen/algebraic_type_variants';

/**
 * The SpacetimeDB Algebraic Type System (SATS) is a structural type system in
 * which a nominal type system can be constructed.
 *
 * The type system unifies the concepts sum types, product types, and built-in
 * primitive types into a single type system.
 */
export type AlgebraicType = AlgebraicTypeType;

/**
 * Algebraic Type utilities.
 */
export const AlgebraicType: {
  createOptionType(innerType: AlgebraicTypeType): AlgebraicTypeType;
  createIdentityType(): AlgebraicTypeType;
  createConnectionIdType(): AlgebraicTypeType;
  createScheduleAtType(): AlgebraicTypeType;
  createTimestampType(): AlgebraicTypeType;
  createTimeDurationType(): AlgebraicTypeType;
  serializeValue(writer: BinaryWriter, ty: AlgebraicTypeType, value: any): void;
  deserializeValue(reader: BinaryReader, ty: AlgebraicTypeType): any;
  /**
   * Convert a value of the algebraic type into something that can be used as a key in a map.
   * There are no guarantees about being able to order it.
   * This is only guaranteed to be comparable to other values of the same type.
   * @param value A value of the algebraic type
   * @returns Something that can be used as a key in a map.
   */
  intoMapKey(ty: AlgebraicTypeType, value: any): ComparablePrimitive;
} & typeof AlgebraicTypeValue = {
  ...AlgebraicTypeValue,
  createOptionType: function (innerType: AlgebraicTypeType): AlgebraicTypeType {
    return AlgebraicTypeValue.Sum({
      variants: [
        { name: 'some', algebraicType: innerType },
        {
          name: 'none',
          algebraicType: AlgebraicTypeValue.Product({ elements: [] }),
        },
      ],
    });
  },
  createIdentityType: function (): AlgebraicTypeType {
    return AlgebraicTypeValue.Product({
      elements: [
        { name: '__identity__', algebraicType: AlgebraicTypeValue.U256 },
      ],
    });
  },
  createConnectionIdType: function (): AlgebraicTypeType {
    return AlgebraicTypeValue.Product({
      elements: [
        { name: '__connection_id__', algebraicType: AlgebraicTypeValue.U128 },
      ],
    });
  },
  createScheduleAtType: function (): AlgebraicTypeType {
    return ScheduleAt.getAlgebraicType();
  },
  createTimestampType: function (): AlgebraicTypeType {
    return AlgebraicTypeValue.Product({
      elements: [
        {
          name: '__timestamp_micros_since_unix_epoch__',
          algebraicType: AlgebraicTypeValue.I64,
        },
      ],
    });
  },
  createTimeDurationType: function (): AlgebraicTypeType {
    return AlgebraicTypeValue.Product({
      elements: [
        {
          name: '__time_duration_micros__',
          algebraicType: AlgebraicTypeValue.I64,
        },
      ],
    });
  },
  serializeValue: function (
    writer: BinaryWriter,
    ty: AlgebraicTypeType,
    value: any
  ): void {
    switch (ty.tag) {
      case 'Product':
        ProductType.serializeValue(writer, ty.value, value);
        break;
      case 'Sum':
        SumType.serializeValue(writer, ty.value, value);
        break;
      case 'Array':
        if (ty.value.tag === 'U8') {
          writer.writeUInt8Array(value);
        } else {
          const elemType = ty.value;
          writer.writeU32(value.length);
          for (const elem of value) {
            AlgebraicType.serializeValue(writer, elemType, elem);
          }
        }
        break;
      case 'Bool':
        writer.writeBool(value);
        break;
      case 'I8':
        writer.writeI8(value);
        break;
      case 'U8':
        writer.writeU8(value);
        break;
      case 'I16':
        writer.writeI16(value);
        break;
      case 'U16':
        writer.writeU16(value);
        break;
      case 'I32':
        writer.writeI32(value);
        break;
      case 'U32':
        writer.writeU32(value);
        break;
      case 'I64':
        writer.writeI64(value);
        break;
      case 'U64':
        writer.writeU64(value);
        break;
      case 'I128':
        writer.writeI128(value);
        break;
      case 'U128':
        writer.writeU128(value);
        break;
      case 'I256':
        writer.writeI256(value);
        break;
      case 'U256':
        writer.writeU256(value);
        break;
      case 'F32':
        writer.writeF32(value);
        break;
      case 'F64':
        writer.writeF64(value);
        break;
      case 'String':
        writer.writeString(value);
        break;
      default:
        throw new Error(`not implemented, ${ty.tag}`);
    }
  },
  deserializeValue: function (
    reader: BinaryReader,
    ty: AlgebraicTypeType
  ): any {
    switch (ty.tag) {
      case 'Product':
        return ProductType.deserializeValue(reader, ty.value);
      case 'Sum':
        return SumType.deserializeValue(reader, ty.value);
      case 'Array':
        if (ty.value.tag === 'U8') {
          return reader.readUInt8Array();
        } else {
          const elemType = ty.value;
          const length = reader.readU32();
          const result: any[] = [];
          for (let i = 0; i < length; i++) {
            result.push(AlgebraicType.deserializeValue(reader, elemType));
          }
          return result;
        }
      case 'Bool':
        return reader.readBool();
      case 'I8':
        return reader.readI8();
      case 'U8':
        return reader.readU8();
      case 'I16':
        return reader.readI16();
      case 'U16':
        return reader.readU16();
      case 'I32':
        return reader.readI32();
      case 'U32':
        return reader.readU32();
      case 'I64':
        return reader.readI64();
      case 'U64':
        return reader.readU64();
      case 'I128':
        return reader.readI128();
      case 'U128':
        return reader.readU128();
      case 'I256':
        return reader.readI256();
      case 'U256':
        return reader.readU256();
      case 'F32':
        return reader.readF32();
      case 'F64':
        return reader.readF64();
      case 'String':
        return reader.readString();
      default:
        throw new Error(`not implemented, ${ty.tag}`);
    }
  },
  /**
   * Convert a value of the algebraic type into something that can be used as a key in a map.
   * There are no guarantees about being able to order it.
   * This is only guaranteed to be comparable to other values of the same type.
   * @param value A value of the algebraic type
   * @returns Something that can be used as a key in a map.
   */
  intoMapKey: function (
    ty: AlgebraicTypeType,
    value: any
  ): ComparablePrimitive {
    switch (ty.tag) {
      case 'U8':
      case 'U16':
      case 'U32':
      case 'U64':
      case 'U128':
      case 'U256':
      case 'I8':
      case 'I16':
      case 'I64':
      case 'I128':
      case 'F32':
      case 'F64':
      case 'String':
      case 'Bool':
        return value;
      case 'Product':
        return ProductType.intoMapKey(ty.value, value);
      default: {
        const writer = new BinaryWriter(10);
        this.serialize(writer, value);
        return writer.toBase64();
      }
    }
  },
};

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
export type ProductType = ProductTypeType;

export const ProductType: {
  serializeValue(writer: BinaryWriter, ty: ProductTypeType, value: any): void;
  deserializeValue(reader: BinaryReader, ty: ProductTypeType): any;
  intoMapKey(ty: ProductTypeType, value: any): ComparablePrimitive;
} = {
  ...ProductTypeValue,
  serializeValue(writer: BinaryWriter, ty: ProductTypeType, value: any): void {
    for (const element of ty.elements) {
      AlgebraicType.serializeValue(
        writer,
        element.algebraicType,
        value[element.name!]
      );
    }
  },
  deserializeValue(reader: BinaryReader, ty: ProductTypeType): any {
    const result: { [key: string]: any } = {};
    if (ty.elements.length === 1) {
      if (ty.elements[0].name === '__time_duration_micros__') {
        return new TimeDuration(reader.readI64());
      }

      if (ty.elements[0].name === '__timestamp_micros_since_unix_epoch__') {
        return new Timestamp(reader.readI64());
      }

      if (ty.elements[0].name === '__identity__') {
        return new Identity(reader.readU256());
      }

      if (ty.elements[0].name === '__connection_id__') {
        return new ConnectionId(reader.readU128());
      }
    }

    for (const element of ty.elements) {
      result[element.name!] = AlgebraicType.deserializeValue(
        reader,
        element.algebraicType
      );
    }
    return result;
  },
  intoMapKey(ty: ProductTypeType, value: any): ComparablePrimitive {
    if (ty.elements.length === 1) {
      if (ty.elements[0].name === '__time_duration_micros__') {
        return (value as TimeDuration).__time_duration_micros__;
      }

      if (ty.elements[0].name === '__timestamp_micros_since_unix_epoch__') {
        return (value as Timestamp).__timestamp_micros_since_unix_epoch__;
      }

      if (ty.elements[0].name === '__identity__') {
        return (value as Identity).__identity__;
      }

      if (ty.elements[0].name === '__connection_id__') {
        return (value as ConnectionId).__connection_id__;
      }
    }
    // The fallback is to serialize and base64 encode the bytes.
    const writer = new BinaryWriter(10);
    AlgebraicType.serializeValue(writer, AlgebraicType.Product(ty), value);
    return writer.toBase64();
  },
};

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
export const SumType: {
  serializeValue(writer: BinaryWriter, ty: SumTypeType, value: any): void;
  deserializeValue(reader: BinaryReader, ty: SumTypeType): any;
} = {
  ...SumTypeValue,
  serializeValue: function (
    writer: BinaryWriter,
    ty: SumTypeType,
    value: any
  ): void {
    if (
      ty.variants.length == 2 &&
      ty.variants[0].name === 'some' &&
      ty.variants[1].name === 'none'
    ) {
      if (value !== null && value !== undefined) {
        writer.writeByte(0);
        AlgebraicType.serializeValue(
          writer,
          ty.variants[0].algebraicType,
          value
        );
      } else {
        writer.writeByte(1);
      }
    } else {
      const variant = value['tag'];
      const index = ty.variants.findIndex(v => v.name === variant);
      if (index < 0) {
        throw `Can't serialize a sum type, couldn't find ${value.tag} tag`;
      }
      writer.writeU8(index);
      AlgebraicType.serializeValue(
        writer,
        ty.variants[index].algebraicType,
        value['value']
      );
    }
  },
  deserializeValue: function (reader: BinaryReader, ty: SumTypeType): any {
    const tag = reader.readU8();
    // In TypeScript we handle Option values as a special case
    // we don't represent the some and none variants, but instead
    // we represent the value directly.
    if (
      ty.variants.length == 2 &&
      ty.variants[0].name === 'some' &&
      ty.variants[1].name === 'none'
    ) {
      if (tag === 0) {
        return AlgebraicType.deserializeValue(
          reader,
          ty.variants[0].algebraicType
        );
      } else if (tag === 1) {
        return undefined;
      } else {
        throw `Can't deserialize an option type, couldn't find ${tag} tag`;
      }
    } else {
      const variant = ty.variants[tag];
      const value = AlgebraicType.deserializeValue(
        reader,
        variant.algebraicType
      );
      return { tag: variant.name, value };
    }
  },
};

export type ComparablePrimitive = number | string | boolean | bigint;
