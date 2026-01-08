import { TimeDuration } from './time_duration';
import { Timestamp } from './timestamp';
import { Uuid } from './uuid';
import { ConnectionId } from './connection_id';
import BinaryReader from './binary_reader';
import BinaryWriter from './binary_writer';
import { Identity } from './identity';
import * as AlgebraicTypeVariants from './algebraic_type_variants';
import { hasOwn } from './util';

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

// Caches to prevent `makeSerializer`/`makeDeserializer` from recursing
// infinitely when called on recursive types. We use `WeakMap` because we don't
// care about iterating these, and it can allow the memory to be freed.
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
          return serializeUint8Array;
        } else {
          const serialize = AlgebraicType.makeSerializer(ty.value, typespace);
          return (writer, value) => {
            writer.writeU32(value.length);
            for (const elem of value) {
              serialize(writer, elem);
            }
          };
        }
      default:
        return primitiveSerializers[ty.tag];
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
          return deserializeUint8Array;
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
      default:
        return primitiveDeserializers[ty.tag];
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

function bindCall<F extends (this: any, ...args: any[]) => any>(
  f: F
): (recv: ThisParameterType<F>, ...args: Parameters<F>) => ReturnType<F> {
  return Function.prototype.call.bind(f);
}

type Primitives = Exclude<
  AlgebraicType['tag'],
  'Ref' | 'Sum' | 'Product' | 'Array'
>;

const primitiveSerializers: Record<Primitives, Serializer<any>> = {
  Bool: bindCall(BinaryWriter.prototype.writeBool),
  I8: bindCall(BinaryWriter.prototype.writeI8),
  U8: bindCall(BinaryWriter.prototype.writeU8),
  I16: bindCall(BinaryWriter.prototype.writeI16),
  U16: bindCall(BinaryWriter.prototype.writeU16),
  I32: bindCall(BinaryWriter.prototype.writeI32),
  U32: bindCall(BinaryWriter.prototype.writeU32),
  I64: bindCall(BinaryWriter.prototype.writeI64),
  U64: bindCall(BinaryWriter.prototype.writeU64),
  I128: bindCall(BinaryWriter.prototype.writeI128),
  U128: bindCall(BinaryWriter.prototype.writeU128),
  I256: bindCall(BinaryWriter.prototype.writeI256),
  U256: bindCall(BinaryWriter.prototype.writeU256),
  F32: bindCall(BinaryWriter.prototype.writeF32),
  F64: bindCall(BinaryWriter.prototype.writeF64),
  String: bindCall(BinaryWriter.prototype.writeString),
};
Object.freeze(primitiveSerializers);

const primitives = new Set(Object.keys(primitiveSerializers));

const serializeUint8Array = bindCall(BinaryWriter.prototype.writeUInt8Array);

const primitiveDeserializers: Record<Primitives, Deserializer<any>> = {
  Bool: bindCall(BinaryReader.prototype.readBool),
  I8: bindCall(BinaryReader.prototype.readI8),
  U8: bindCall(BinaryReader.prototype.readU8),
  I16: bindCall(BinaryReader.prototype.readI16),
  U16: bindCall(BinaryReader.prototype.readU16),
  I32: bindCall(BinaryReader.prototype.readI32),
  U32: bindCall(BinaryReader.prototype.readU32),
  I64: bindCall(BinaryReader.prototype.readI64),
  U64: bindCall(BinaryReader.prototype.readU64),
  I128: bindCall(BinaryReader.prototype.readI128),
  U128: bindCall(BinaryReader.prototype.readU128),
  I256: bindCall(BinaryReader.prototype.readI256),
  U256: bindCall(BinaryReader.prototype.readU256),
  F32: bindCall(BinaryReader.prototype.readF32),
  F64: bindCall(BinaryReader.prototype.readF64),
  String: bindCall(BinaryReader.prototype.readString),
};
Object.freeze(primitiveDeserializers);

const deserializeUint8Array = bindCall(BinaryReader.prototype.readUInt8Array);

type FixedSizePrimitives = Exclude<Primitives, 'String'>;

const primitiveSizes: Record<FixedSizePrimitives, number> = {
  Bool: 1,
  I8: 1,
  U8: 1,
  I16: 2,
  U16: 2,
  I32: 4,
  U32: 4,
  I64: 8,
  U64: 8,
  I128: 16,
  U128: 16,
  I256: 32,
  U256: 32,
  F32: 4,
  F64: 8,
};

const fixedSizePrimitives = new Set(Object.keys(primitiveSizes));

type FixedSizeProductType = {
  elements: { name: string; algebraicType: { tag: FixedSizePrimitives } }[];
};

const isFixedSizeProduct = (ty: ProductType): ty is FixedSizeProductType =>
  ty.elements.every(({ algebraicType }) =>
    fixedSizePrimitives.has(algebraicType.tag)
  );

const productSize = (ty: FixedSizeProductType): number =>
  ty.elements.reduce(
    (acc, { algebraicType }) => acc + primitiveSizes[algebraicType.tag],
    0
  );

const primitiveJSName: Record<FixedSizePrimitives, string> = {
  Bool: 'Uint8',
  I8: 'Int8',
  U8: 'Uint8',
  I16: 'Int16',
  U16: 'Uint16',
  I32: 'Int32',
  U32: 'Uint32',
  I64: 'Int64',
  U64: 'Uint64',
  I128: 'Int128',
  U128: 'Uint128',
  I256: 'Int256',
  U256: 'Uint256',
  F32: 'Float32',
  F64: 'Float64',
};

type SpecialProducts = {
  __time_duration_micros__: TimeDuration;
  __timestamp_micros_since_unix_epoch__: Timestamp;
  __identity__: Identity;
  __connection_id__: ConnectionId;
  __uuid__: Uuid;
};

const specialProductDeserializers: {
  [k in keyof SpecialProducts]: Deserializer<SpecialProducts[k]>;
} = {
  __time_duration_micros__: reader => new TimeDuration(reader.readI64()),
  __timestamp_micros_since_unix_epoch__: reader =>
    new Timestamp(reader.readI64()),
  __identity__: reader => new Identity(reader.readU256()),
  __connection_id__: reader => new ConnectionId(reader.readU128()),
  __uuid__: reader => new Uuid(reader.readU128()),
};
Object.freeze(specialProductDeserializers);

const unitDeserializer: Deserializer<{}> = () => ({});

const getElementInitializer = (element: ProductTypeElement) => {
  let init: string;
  switch (element.algebraicType.tag) {
    case 'String':
      init = "''";
      break;
    case 'Bool':
      init = 'false';
      break;
    case 'I8':
    case 'U8':
    case 'I16':
    case 'U16':
    case 'I32':
    case 'U32':
      init = '0';
      break;
    case 'I64':
    case 'U64':
    case 'I128':
    case 'U128':
    case 'I256':
    case 'U256':
      init = '0n';
      break;
    case 'F32':
    case 'F64':
      init = '0.0';
      break;
    default:
      init = 'undefined';
  }
  return `${element.name!}: ${init}`;
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
    if (serializer !== undefined) return serializer;

    if (isFixedSizeProduct(ty)) {
      const size = productSize(ty);
      const body = `\
"use strict";
writer.expandBuffer(${size});
const view = writer.buffer.view;
${ty.elements
  .map(
    ({ name, algebraicType: { tag } }) => `\
view.set${primitiveJSName[tag]}(value.${name!}, writer.offset, ${primitiveSizes[tag] > 1 ? 'true' : ''});
writer.offset += ${primitiveSizes[tag]};`
  )
  .join('\n')}`;
      serializer = Function('writer', 'value', body) as Serializer<any>;
    }

    const primitiveFields = new Set();
    const serializers: Record<string, Serializer<any>> = {};
    const body =
      '"use strict";\n' +
      ty.elements
        .map(element => {
          if (primitives.has(element.algebraicType.tag)) {
            primitiveFields.add(element.name!);
            return `writer.write${element.algebraicType.tag}(value.${element.name!});`;
          } else {
            return `serializers.${element.name!}(writer, value.${element.name!});`;
          }
        })
        .join('\n');
    serializer = Function('serializers', 'writer', 'value', body).bind(
      undefined,
      serializers
    ) as Serializer<any>;
    // In case `ty` is recursive, we cache the function *before* before computing
    // `serializers`, so that a recursive `makeSerializer` with the same `ty` has
    // an exit condition.
    SERIALIZERS.set(ty, serializer);
    for (const { name, algebraicType } of ty.elements) {
      if (primitiveFields.has(name)) continue;
      serializers[name!] = AlgebraicType.makeSerializer(
        algebraicType,
        typespace
      );
    }
    Object.freeze(serializers);
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
    switch (ty.elements.length) {
      case 0:
        return unitDeserializer;
      case 1: {
        const fieldName = ty.elements[0].name!;
        if (hasOwn(specialProductDeserializers, fieldName))
          return specialProductDeserializers[
            fieldName as keyof SpecialProducts
          ];
      }
    }

    let deserializer = DESERIALIZERS.get(ty);
    if (deserializer !== undefined) return deserializer;

    if (isFixedSizeProduct(ty)) {
      // const size = productSize(ty);
      const body = `\
"use strict";
const result = { ${ty.elements.map(getElementInitializer).join(', ')} };
const view = reader.buffer.view;
${ty.elements
  .map(
    ({ name, algebraicType: { tag } }) => `\
result.${name} = view.get${primitiveJSName[tag]}(reader.offset, ${primitiveSizes[tag] > 1 ? 'true' : ''});
reader.offset += ${primitiveSizes[tag]};`
  )
  .join('\n')}
return result;`;
      deserializer = Function('reader', body) as Deserializer<any>;
    }

    const deserializers: Record<string, Deserializer<any>> = {};
    deserializer = Function(
      'deserializers',
      'reader',
      `\
"use strict";
const result = { ${ty.elements.map(getElementInitializer).join(', ')} };
${ty.elements.map(({ name }) => `result.${name!} = deserializers.${name!}(reader);`).join('\n')}
return result;`
    ).bind(undefined, deserializers) as Deserializer<any>;
    // In case `ty` is recursive, we cache the function *before* before computing
    // `deserializers`, so that a recursive `makeDeserializer` with the same `ty` has
    // an exit condition.
    DESERIALIZERS.set(ty, deserializer);
    for (const { name, algebraicType } of ty.elements) {
      deserializers[name!] = AlgebraicType.makeDeserializer(
        algebraicType,
        typespace
      );
    }
    Object.freeze(deserializers);
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
      const fieldName = ty.elements[0].name!;
      if (hasOwn(specialProductDeserializers, fieldName)) {
        return value[fieldName];
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
      if (serializer != undefined) return serializer;
      serializer = (writer, value) => {
        const variant = variants.get(value.tag);
        if (variant === undefined) {
          throw `Can't serialize a sum type, couldn't find ${value.tag} tag ${JSON.stringify(value)} in variants ${JSON.stringify([...variants.keys()])}`;
        }
        const { index, serialize } = variant;
        writer.writeU8(index);
        serialize(writer, value.value);
      };
      // In case `ty` is recursive, we cache the function *before* before computing
      // `variants`, so that a recursive `makeSerializer` with the same `ty` has
      // an exit condition.
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
    } else if (
      ty.variants.length == 2 &&
      ty.variants[0].name === 'ok' &&
      ty.variants[1].name === 'err'
    ) {
      const deserializeOk = AlgebraicType.makeDeserializer(
        ty.variants[0].algebraicType,
        typespace
      );
      const deserializeErr = AlgebraicType.makeDeserializer(
        ty.variants[1].algebraicType,
        typespace
      );
      return reader => {
        const tag = reader.readU8();
        if (tag === 0) {
          return deserializeOk(reader);
        } else if (tag === 1) {
          return deserializeErr(reader);
        } else {
          throw `Can't deserialize a result type, couldn't find ${tag} tag`;
        }
      };
    } else {
      let deserializer = DESERIALIZERS.get(ty);
      if (deserializer !== undefined) return deserializer;
      const deserializers: Record<string, Deserializer<any>> = {};
      deserializer = Function(
        'deserializers',
        'reader',
        `switch (reader.readU8()) {\n${ty.variants
          .map(
            ({ name }, i) =>
              `case ${i}: return { tag: ${JSON.stringify(name!)}, value: deserializers.${name!}(reader) };`
          )
          .join('\n')} }`
      ).bind(undefined, deserializers) as Deserializer<any>;
      // In case `ty` is recursive, we cache the function *before* before computing
      // `deserializers`, so that a recursive `makeDeserializer` with the same `ty` has
      // an exit condition.
      DESERIALIZERS.set(ty, deserializer);
      for (const { name, algebraicType } of ty.variants) {
        deserializers[name!] = AlgebraicType.makeDeserializer(
          algebraicType,
          typespace
        );
      }
      Object.freeze(deserializers);
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
