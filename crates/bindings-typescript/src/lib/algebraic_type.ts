import { TimeDuration } from './time_duration';
import { Timestamp } from './timestamp';
import { ConnectionId } from './connection_id';
import type BinaryReader from './binary_reader';
import BinaryWriter from './binary_writer';
import { Identity } from './identity';
import * as AlgebraicTypeVariants from './algebraic_type_variants';

type TypespaceType = {
  types: AlgebraicTypeType[];
};

export type ProductTypeType = {
  elements: ProductTypeElement[];
};

/**
 * A factor / element of a product type.
 *
 * An element consist of an optional name and a type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export type ProductTypeElement = {
  name: string | undefined;
  algebraicType: AlgebraicTypeType;
};

export type SumTypeType = {
  variants: SumTypeVariant[];
};

/**
 * A variant of a sum type.
 *
 * NOTE: Each element has an implicit element tag based on its order.
 * Uniquely identifies an element similarly to protobuf tags.
 */
export type SumTypeVariant = {
  name: string | undefined;
  algebraicType: AlgebraicTypeType;
};

export type AlgebraicTypeType =
  | AlgebraicTypeVariants.Ref
  | AlgebraicTypeVariants.Sum
  | AlgebraicTypeVariants.Product
  | AlgebraicTypeVariants.Array
  | AlgebraicTypeVariants.String
  | AlgebraicTypeVariants.Bool
  | AlgebraicTypeVariants.I8
  | AlgebraicTypeVariants.U8
  | AlgebraicTypeVariants.I16
  | AlgebraicTypeVariants.U16
  | AlgebraicTypeVariants.I32
  | AlgebraicTypeVariants.U32
  | AlgebraicTypeVariants.I64
  | AlgebraicTypeVariants.U64
  | AlgebraicTypeVariants.I128
  | AlgebraicTypeVariants.U128
  | AlgebraicTypeVariants.I256
  | AlgebraicTypeVariants.U256
  | AlgebraicTypeVariants.F32
  | AlgebraicTypeVariants.F64;

export type AlgebraicType = AlgebraicTypeType;

/**
 * The variant types of the Algebraic Type tagged union.
 */
export { AlgebraicTypeVariants };

export type Serializer<T> = (writer: BinaryWriter, value: T) => void;

export type Deserializer<T> = (reader: BinaryReader) => T;

const SERIALIZERS = new WeakMap<ProductType | SumType, Serializer<any>>();
const DESERIALIZERS = new WeakMap<ProductType | SumType, Deserializer<any>>();

// A value with helper functions to construct the type.
export const AlgebraicType = {
  Ref: (value: number): AlgebraicTypeVariants.Ref => ({ tag: 'Ref', value }),
  Sum: <T extends SumTypeType>(value: T): { tag: 'Sum'; value: T } => ({
    tag: 'Sum',
    value,
  }),
  Product: <T extends ProductTypeType>(
    value: T
  ): { tag: 'Product'; value: T } => ({
    tag: 'Product',
    value,
  }),
  Array: <T extends AlgebraicTypeType>(
    value: T
  ): { tag: 'Array'; value: T } => ({
    tag: 'Array',
    value,
  }),
  String: { tag: 'String' } as const,
  Bool: { tag: 'Bool' } as const,
  I8: { tag: 'I8' } as const,
  U8: { tag: 'U8' } as const,
  I16: { tag: 'I16' } as const,
  U16: { tag: 'U16' } as const,
  I32: { tag: 'I32' } as const,
  U32: { tag: 'U32' } as const,
  I64: { tag: 'I64' } as const,
  U64: { tag: 'U64' } as const,
  I128: { tag: 'I128' } as const,
  U128: { tag: 'U128' } as const,
  I256: { tag: 'I256' } as const,
  U256: { tag: 'U256' } as const,
  F32: { tag: 'F32' } as const,
  F64: { tag: 'F64' } as const,
  makeSerializer(
    ty: AlgebraicTypeType,
    typespace?: TypespaceType
  ): Serializer<any> {
    if (ty.tag === 'Ref') {
      if (!typespace)
        throw new Error('cannot serialize refs without a typespace');
      while (ty.tag === 'Ref') ty = typespace.types[ty.value];
    }
    switch (ty.tag) {
      case 'Product':
        return ProductType.makeSerializer(ty.value, typespace);
      case 'Sum':
        return SumType.makeSerializer(ty.value, typespace);
      case 'Array':
        if (ty.value.tag === 'U8') {
          return (writer, value) => writer.writeUInt8Array(value);
        } else {
          const serialize = AlgebraicType.makeSerializer(ty.value, typespace);
          return (writer, value) => {
            writer.writeU32(value.length);
            for (const elem of value) {
              serialize(writer, elem);
            }
          };
        }
      case 'Bool':
        return (writer, value) => writer.writeBool(value);
      case 'I8':
        return (writer, value) => writer.writeI8(value);
      case 'U8':
        return (writer, value) => writer.writeU8(value);
      case 'I16':
        return (writer, value) => writer.writeI16(value);
      case 'U16':
        return (writer, value) => writer.writeU16(value);
      case 'I32':
        return (writer, value) => writer.writeI32(value);
      case 'U32':
        return (writer, value) => writer.writeU32(value);
      case 'I64':
        return (writer, value) => writer.writeI64(value);
      case 'U64':
        return (writer, value) => writer.writeU64(value);
      case 'I128':
        return (writer, value) => writer.writeI128(value);
      case 'U128':
        return (writer, value) => writer.writeU128(value);
      case 'I256':
        return (writer, value) => writer.writeI256(value);
      case 'U256':
        return (writer, value) => writer.writeU256(value);
      case 'F32':
        return (writer, value) => writer.writeF32(value);
      case 'F64':
        return (writer, value) => writer.writeF64(value);
      case 'String':
        return (writer, value) => writer.writeString(value);
    }
  },
  serializeValue(
    writer: BinaryWriter,
    ty: AlgebraicTypeType,
    value: any,
    typespace?: TypespaceType
  ) {
    AlgebraicType.makeSerializer(ty, typespace)(writer, value);
  },
  makeDeserializer(
    ty: AlgebraicTypeType,
    typespace?: TypespaceType
  ): Deserializer<any> {
    if (ty.tag === 'Ref') {
      if (!typespace)
        throw new Error('cannot deserialize refs without a typespace');
      while (ty.tag === 'Ref') ty = typespace.types[ty.value];
    }
    switch (ty.tag) {
      case 'Product':
        return ProductType.makeDeserializer(ty.value, typespace);
      case 'Sum':
        return SumType.makeDeserializer(ty.value, typespace);
      case 'Array':
        if (ty.value.tag === 'U8') {
          return reader => reader.readUInt8Array();
        } else {
          const deserialize = AlgebraicType.makeDeserializer(
            ty.value,
            typespace
          );
          return reader => {
            const length = reader.readU32();
            const result: any[] = Array(length);
            for (let i = 0; i < length; i++) {
              result[i] = deserialize(reader);
            }
            return result;
          };
        }
      case 'Bool':
        return reader => reader.readBool();
      case 'I8':
        return reader => reader.readI8();
      case 'U8':
        return reader => reader.readU8();
      case 'I16':
        return reader => reader.readI16();
      case 'U16':
        return reader => reader.readU16();
      case 'I32':
        return reader => reader.readI32();
      case 'U32':
        return reader => reader.readU32();
      case 'I64':
        return reader => reader.readI64();
      case 'U64':
        return reader => reader.readU64();
      case 'I128':
        return reader => reader.readI128();
      case 'U128':
        return reader => reader.readU128();
      case 'I256':
        return reader => reader.readI256();
      case 'U256':
        return reader => reader.readU256();
      case 'F32':
        return reader => reader.readF32();
      case 'F64':
        return reader => reader.readF64();
      case 'String':
        return reader => reader.readString();
    }
  },
  deserializeValue(
    reader: BinaryReader,
    ty: AlgebraicTypeType,
    typespace?: TypespaceType
  ): any {
    return AlgebraicType.makeDeserializer(ty, typespace)(reader);
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
      case 'I32':
      case 'I64':
      case 'I128':
      case 'I256':
      case 'F32':
      case 'F64':
      case 'String':
      case 'Bool':
        return value;
      case 'Product':
        return ProductType.intoMapKey(ty.value, value);
      default: {
        // The fallback is to serialize and base64 encode the bytes.
        const writer = new BinaryWriter(10);
        AlgebraicType.serializeValue(writer, ty, value);
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

export const ProductType = {
  makeSerializer(
    ty: ProductTypeType,
    typespace?: TypespaceType
  ): Serializer<any> {
    let serializer = SERIALIZERS.get(ty);
    if (serializer != null) return serializer;
    serializer = (writer, value) => {
      for (const { name, serialize } of elements) {
        serialize(writer, value[name]);
      }
    };
    SERIALIZERS.set(ty, serializer);
    const elements = ty.elements.map(element => ({
      name: element.name!,
      serialize: AlgebraicType.makeSerializer(element.algebraicType, typespace),
    }));
    return serializer;
  },
  serializeValue(
    writer: BinaryWriter,
    ty: ProductTypeType,
    value: any,
    typespace?: TypespaceType
  ): void {
    ProductType.makeSerializer(ty, typespace)(writer, value);
  },
  makeDeserializer(
    ty: ProductTypeType,
    typespace?: TypespaceType
  ): Deserializer<any> {
    if (ty.elements.length === 1) {
      if (ty.elements[0].name === '__time_duration_micros__') {
        return reader => new TimeDuration(reader.readI64());
      }

      if (ty.elements[0].name === '__timestamp_micros_since_unix_epoch__') {
        return reader => new Timestamp(reader.readI64());
      }

      if (ty.elements[0].name === '__identity__') {
        return reader => new Identity(reader.readU256());
      }

      if (ty.elements[0].name === '__connection_id__') {
        return reader => new ConnectionId(reader.readU128());
      }
    }

    let deserializer = DESERIALIZERS.get(ty);
    if (deserializer != null) return deserializer;
    deserializer = reader => {
      const result: { [key: string]: any } = {};
      for (const { name, deserialize } of elements) {
        result[name] = deserialize(reader);
      }
      return result;
    };
    DESERIALIZERS.set(ty, deserializer);
    const elements = ty.elements.map(element => ({
      name: element.name!,
      deserialize: AlgebraicType.makeDeserializer(
        element.algebraicType,
        typespace
      ),
    }));
    return deserializer;
  },
  deserializeValue(
    reader: BinaryReader,
    ty: ProductTypeType,
    typespace?: TypespaceType
  ): any {
    return ProductType.makeDeserializer(ty, typespace)(reader);
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

export type SumType = SumTypeType;

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
export const SumType = {
  makeSerializer(ty: SumTypeType, typespace?: TypespaceType): Serializer<any> {
    if (
      ty.variants.length == 2 &&
      ty.variants[0].name === 'some' &&
      ty.variants[1].name === 'none'
    ) {
      const serialize = AlgebraicType.makeSerializer(
        ty.variants[0].algebraicType,
        typespace
      );
      return (writer, value) => {
        if (value !== null && value !== undefined) {
          writer.writeByte(0);
          serialize(writer, value);
        } else {
          writer.writeByte(1);
        }
      };
    } else {
      let serializer = SERIALIZERS.get(ty);
      if (serializer != null) return serializer;
      serializer = (writer, value) => {
        const variant = variants.get(value.tag);
        if (variant == null) {
          throw `Can't serialize a sum type, couldn't find ${value.tag} tag ${JSON.stringify(value)} in variants ${JSON.stringify([...variants.keys()])}`;
        }
        const { index, serialize } = variant;
        writer.writeU8(index);
        serialize(writer, value.value);
      };
      SERIALIZERS.set(ty, serializer);
      const variants = new Map(
        ty.variants.map((element, index) => [
          element.name!,
          {
            index,
            serialize: AlgebraicType.makeSerializer(
              element.algebraicType,
              typespace
            ),
          },
        ])
      );
      return serializer;
    }
  },
  serializeValue(
    writer: BinaryWriter,
    ty: SumTypeType,
    value: any,
    typespace?: TypespaceType
  ): void {
    SumType.makeSerializer(ty, typespace)(writer, value);
  },
  makeDeserializer(
    ty: SumTypeType,
    typespace?: TypespaceType
  ): Deserializer<any> {
    // In TypeScript we handle Option values as a special case
    // we don't represent the some and none variants, but instead
    // we represent the value directly.
    if (
      ty.variants.length == 2 &&
      ty.variants[0].name === 'some' &&
      ty.variants[1].name === 'none'
    ) {
      const deserialize = AlgebraicType.makeDeserializer(
        ty.variants[0].algebraicType,
        typespace
      );
      return reader => {
        const tag = reader.readU8();
        if (tag === 0) {
          return deserialize(reader);
        } else if (tag === 1) {
          return undefined;
        } else {
          throw `Can't deserialize an option type, couldn't find ${tag} tag`;
        }
      };
    } else {
      let deserializer = DESERIALIZERS.get(ty);
      if (deserializer != null) return deserializer;
      deserializer = reader => {
        const tag = reader.readU8();
        const { name, deserialize } = variants[tag];
        const value = deserialize(reader);
        return { tag: name, value };
      };
      DESERIALIZERS.set(ty, deserializer);
      const variants = ty.variants.map(variant => ({
        name: variant.name!,
        deserialize: AlgebraicType.makeDeserializer(
          variant.algebraicType,
          typespace
        ),
      }));
      return deserializer;
    }
  },
  deserializeValue(
    reader: BinaryReader,
    ty: SumTypeType,
    typespace?: TypespaceType
  ): any {
    return SumType.makeDeserializer(ty, typespace)(reader);
  },
};

export type ComparablePrimitive = number | string | boolean | bigint;
