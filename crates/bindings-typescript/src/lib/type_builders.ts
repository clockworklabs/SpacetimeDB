import { AlgebraicType, type AlgebraicTypeVariants } from './algebraic_type';
import type BinaryReader from './binary_reader';
import type BinaryWriter from './binary_writer';
import { ConnectionId, type ConnectionIdAlgebraicType } from './connection_id';
import { Identity, type IdentityAlgebraicType } from './identity';
import { Option, type OptionAlgebraicType } from './option';
import { Result, type ResultAlgebraicType } from './result';
import ScheduleAt, { type ScheduleAtAlgebraicType } from './schedule_at';
import type { CoerceRow } from './table';
import { TimeDuration, type TimeDurationAlgebraicType } from './time_duration';
import { Timestamp, type TimestampAlgebraicType } from './timestamp';
import { set, type Prettify, type SetField } from './type_util';
import { Uuid, type UuidAlgebraicType } from './uuid';

// Used in codegen files
export { type AlgebraicTypeType } from './algebraic_type';

/**
 * Helper type to extract the TypeScript type from a TypeBuilder
 */
export type InferTypeOfTypeBuilder<T extends TypeBuilder<any, any>> =
  T extends TypeBuilder<infer U, any> ? Prettify<U> : never;

/**
 * Helper type to extract the Spacetime type from a TypeBuilder
 */
export type InferSpacetimeTypeOfTypeBuilder<T extends TypeBuilder<any, any>> =
  T extends TypeBuilder<any, infer U> ? U : never;

/**
 * Helper type to extract the TypeScript type from a TypeBuilder
 */
export type Infer<T> = T extends RowObj
  ? InferTypeOfRow<T>
  : T extends TypeBuilder<any, any>
    ? InferTypeOfTypeBuilder<T>
    : never;

/**
 * Helper type to extract the type of a row from an object.
 */
export type InferTypeOfRow<T extends RowObj> = {
  [K in keyof T & string]: InferTypeOfTypeBuilder<CollapseColumn<T[K]>>;
};

/**
 * Helper type to extract the type of a row from an object.
 */
export type InferSpacetimeTypeOfRow<T extends RowObj> = {
  [K in keyof T & string]: InferSpacetimeTypeOfTypeBuilder<
    CollapseColumn<T[K]>
  >;
};

/**
 * Helper type to extract the Spacetime type from a row object.
 */
type CollapseColumn<
  T extends TypeBuilder<any, any> | ColumnBuilder<any, any, any>,
> = T extends ColumnBuilder<any, any, any> ? T['typeBuilder'] : T;

/**
 * A type representing an object which is used to define the type of
 * a row in a table.
 */
export type RowObj = Record<
  string,
  TypeBuilder<any, any> | ColumnBuilder<any, any, ColumnMetadata<any>>
>;

/**
 * Type which converts the elements of RowObj to a ProductType elements array
 */
type ElementsArrayFromRowObj<Obj extends RowObj> = Array<
  {
    [N in keyof Obj & string]: {
      name: N;
      algebraicType: InferSpacetimeTypeOfTypeBuilder<CollapseColumn<Obj[N]>>;
    };
  }[keyof Obj & string]
>;

/**
 * A type which converts the elements of RowObj to a TypeScript object type.
 * It works by `Infer`ing the types of the column builders which are the values of
 * the keys in the object passed in.
 *
 * e.g. { a: I32TypeBuilder, b: StringBuilder } -> { a: number, b: string }
 */
type RowType<Row extends RowObj> = {
  [K in keyof Row]: InferTypeOfTypeBuilder<CollapseColumn<Row[K]>>;
};

/**
 * Type which represents a valid argument to the ProductColumnBuilder
 */
export type ElementsObj = Record<string, TypeBuilder<any, any>>;

/**
 * Type which converts the elements of ElementsObj to a ProductType elements array
 */
type ElementsArrayFromElementsObj<Obj extends ElementsObj> = Array<
  {
    [N in keyof Obj & string]: {
      name: N;
      algebraicType: InferSpacetimeTypeOfTypeBuilder<Obj[N]>;
    };
  }[keyof Obj & string]
>;

/**
 * A type which converts the elements of ElementsObj to a TypeScript object type.
 * It works by `Infer`ing the types of the column builders which are the values of
 * the keys in the object passed in.
 *
 * e.g. { a: I32TypeBuilder, b: StringBuilder } -> { a: number, b: string }
 */
type ObjectType<Elements extends ElementsObj> = {
  [K in keyof Elements]: InferTypeOfTypeBuilder<Elements[K]>;
};

export type VariantsObj = Record<string, TypeBuilder<any, any>>;
type SimpleVariantsObj = Record<string, UnitBuilder>;

type IsUnit<B> = B extends UnitBuilder ? true : false;

/**
 * A type which converts the elements of ElementsObj to a TypeScript object type.
 * It works by `Infer`ing the types of the column builders which are the values of
 * the keys in the object passed in.
 *
 * e.g. { A: I32TypeBuilder, B: StringBuilder } -> { tag: "A", value: number } | { tag: "B", value: string }
 */
type EnumType<Variants extends VariantsObj> = {
  [K in keyof Variants & string]: IsUnit<Variants[K]> extends true
    ? { tag: K }
    : { tag: K; value: InferTypeOfTypeBuilder<Variants[K]> };
}[keyof Variants & string];

/**
 * Type which converts the elements of VariantsObj to a SumType variants array
 */
type VariantsArrayFromVariantsObj<Obj extends VariantsObj> = {
  name: keyof Obj & string;
  algebraicType: InferSpacetimeTypeOfTypeBuilder<Obj[keyof Obj & string]>;
}[];

/**
 * A generic type builder that captures both the TypeScript type
 * and the corresponding `AlgebraicType`.
 */
export class TypeBuilder<Type, SpacetimeType extends AlgebraicType>
  implements Optional<Type, SpacetimeType>
{
  /**
   * The TypeScript phantom type. This is not stored at runtime,
   * but is visible to the compiler
   */
  readonly type!: Type;

  /**
   * The SpacetimeDB algebraic type (run‑time value). In addition to storing
   * the runtime representation of the `AlgebraicType`, it also captures
   * the TypeScript type information of the `AlgebraicType`. That is to say
   * the value is not merely an `AlgebraicType`, but is constructed to be
   * the corresponding concrete `AlgebraicType` for the TypeScript type `Type`.
   *
   * e.g. `string` corresponds to `AlgebraicType.String`
   */
  readonly algebraicType: SpacetimeType;

  constructor(algebraicType: SpacetimeType) {
    this.algebraicType = algebraicType;
  }

  optional(): OptionBuilder<typeof this> {
    return new OptionBuilder(this);
  }

  serialize(writer: BinaryWriter, value: Type): void {
    const serialize = (this.serialize = AlgebraicType.makeSerializer(
      this.algebraicType
    ));
    serialize(writer, value);
  }

  deserialize(reader: BinaryReader): Type {
    const deserialize = (this.deserialize = AlgebraicType.makeDeserializer(
      this.algebraicType
    ));
    return deserialize(reader);
  }
}

/**
 * Interface for types that can be converted into a column builder with primary key metadata.
 *
 * Implementing this interface allows a type to be marked as the primary key of a table column
 * in a type-safe manner. The `primaryKey()` method returns a new `ColumnBuilder` instance
 * with the metadata updated to indicate that the column is a primary key.
 *
 * @typeParam Type - The TypeScript type of the column's value.
 * @typeParam SpacetimeType - The corresponding SpacetimeDB algebraic type.
 * @typeParam M - The metadata type for the column, defaulting to `DefaultMetadata`.
 *
 * @remarks
 * - This interface is typically implemented by type builders for primitive and complex types.
 * - The returned `ColumnBuilder` will have its metadata extended with `{ isPrimaryKey: true }`.
 * - **Cannot be combined with `default()`.**
 */
interface PrimaryKeyable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify this column as primary key
   * @remarks Cannot be combined with `default()`.
   */
  primaryKey(): ColumnBuilder<
    Type,
    SpacetimeType,
    SetField<M, 'isPrimaryKey', true>
  >;
}

/**
 * Interface for types that can be converted into a column builder with unique metadata.
 *
 * Implementing this interface allows a type to be marked as unique in a table column
 * in a type-safe manner. The `unique()` method returns a new `ColumnBuilder` instance
 * with the metadata updated to indicate that the column is unique.
 *
 * @typeParam Type - The TypeScript type of the column's value.
 * @typeParam SpacetimeType - The corresponding SpacetimeDB algebraic type.
 * @typeParam M - The metadata type for the column, defaulting to `DefaultMetadata`.
 *
 * @remarks
 * - This interface is typically implemented by type builders for primitive and complex types.
 * - The returned `ColumnBuilder` will have its metadata extended with `{ isUnique: true }`.
 * - **Cannot be combined with `default()`.**
 */
interface Uniqueable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify this column as unique
   * @remarks Cannot be combined with `default()`.
   */
  unique(): ColumnBuilder<Type, SpacetimeType, SetField<M, 'isUnique', true>>;
}

/**
 * Interface for types that can be converted into a column builder with index metadata.
 *
 * Implementing this interface allows a type to be indexed in a table column
 * in a type-safe manner. The `index()` method returns a new `ColumnBuilder` instance
 * with the metadata updated to indicate the index type.
 *
 * @typeParam Type - The TypeScript type of the column's value.
 * @typeParam SpacetimeType - The corresponding SpacetimeDB algebraic type.
 * @typeParam M - The metadata type for the column, defaulting to `DefaultMetadata`.
 *
 * @remarks
 * - This interface is typically implemented by type builders for primitive and complex types.
 * - The returned `ColumnBuilder` will have its metadata extended with `{ indexType: N }`.
 * - Indexing a column may have implications for performance and query optimization.
 */
interface Indexable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify the index type for this column
   * @param algorithm The index algorithm to use
   */
  index(): ColumnBuilder<
    Type,
    SpacetimeType,
    SetField<M, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): ColumnBuilder<Type, SpacetimeType, SetField<M, 'indexType', N>>;
}

/**
 * Interface for types that can be converted into a column builder with auto-increment metadata.
 *
 * Implementing this interface allows a type to be marked as auto-incrementing in a table column
 * in a type-safe manner. The `autoInc()` method returns a new `ColumnBuilder` instance
 * with the metadata updated to indicate that the column is auto-incrementing.
 *
 * @typeParam Type - The TypeScript type of the column's value.
 * @typeParam SpacetimeType - The corresponding SpacetimeDB algebraic type.
 * @typeParam M - The metadata type for the column, defaulting to `DefaultMetadata`.
 *
 * @remarks
 * - This interface is typically implemented by type builders for primitive and complex types.
 * - The returned `ColumnBuilder` will have its metadata extended with `{ isAutoIncrement: true }`.
 * - **Cannot be combined with `default()`.**
 */
interface AutoIncrementable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify this column as auto-incrementing
   * @remarks Cannot be combined with `default()`.
   */
  autoInc(): ColumnBuilder<
    Type,
    SpacetimeType,
    SetField<M, 'isAutoIncrement', true>
  >;
}

/**
 * Interface for types that can be converted into an optional type.
 * All {@link TypeBuilder}s implement this interface, however since the `optional()` method
 * returns an {@link OptionBuilder}, {@link OptionBuilder} controls what metadata is allowed
 * to be configured for the column. This allows us to restrict whether things like indexes
 * or unique constraints can be applied to optional columns.
 *
 * For this reason {@link ColumnBuilder} does not implement this interface.
 */
interface Optional<Type, SpacetimeType extends AlgebraicType> {
  /**
   * Specify this column as optional
   */
  optional(this: TypeBuilder<Type, SpacetimeType>): OptionBuilder<typeof this>;
}

/**
 * Interface for types that can be converted into a column builder with default value metadata.
 * Implementing this interface allows a type to have a default value specified in a table column
 * in a type-safe manner. The `default()` method returns a new `ColumnBuilder` instance
 * with the metadata updated to include the specified default value.
 *
 * @typeParam Type - The TypeScript type of the column's value.
 * @typeParam SpacetimeType - The corresponding SpacetimeDB algebraic type.
 * @typeParam M - The metadata type for the column, defaulting to `DefaultMetadata`.
 *
 * @remarks
 * - This interface is typically implemented by type builders for primitive and complex types.
 * - The returned `ColumnBuilder` will have its metadata extended with `{ default: value }`.
 * - The default value must be of the same type as the column's TypeScript type.
 * - This method can be called multiple times; the last call takes precedence.
 * - **Cannot be combined with `primaryKey()`, `unique()`, or `autoInc()`.**
 */
interface Defaultable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify a default value for this column
   * @param value The default value for the column
   * @example
   * ```typescript
   * const col = t.i32().default(42);
   * ```
   * @remarks
   * - This method can be called multiple times; the last call takes precedence.
   * - The default value must be of the same type as the column's TypeScript type.
   * - Cannot be combined with `primaryKey()`, `unique()`, or `autoInc()`.
   */
  default(
    value: Type
  ): ColumnBuilder<Type, SpacetimeType, SetField<M, 'defaultValue', Type>>;
}

interface Nameable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify the in-database name for this column.
   */
  name<const Name extends string>(
    name: Name
  ): Nameable<Type, SpacetimeType, SetField<M, 'name', Name>>;
}

export class U8Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U8>
  implements
    Indexable<number, AlgebraicTypeVariants.U8>,
    Uniqueable<number, AlgebraicTypeVariants.U8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U8>,
    AutoIncrementable<number, AlgebraicTypeVariants.U8>,
    Defaultable<number, AlgebraicTypeVariants.U8>,
    Nameable<number, AlgebraicTypeVariants.U8>
{
  constructor() {
    super(AlgebraicType.U8);
  }
  index(): U8ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U8ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U8ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U8ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new U8ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U8ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U8ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U8ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U8ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new U8ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class U16Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U16>
  implements
    Indexable<number, AlgebraicTypeVariants.U16>,
    Uniqueable<number, AlgebraicTypeVariants.U16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U16>,
    AutoIncrementable<number, AlgebraicTypeVariants.U16>,
    Defaultable<number, AlgebraicTypeVariants.U16>,
    Nameable<number, AlgebraicTypeVariants.U16>
{
  constructor() {
    super(AlgebraicType.U16);
  }
  index(): U16ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U16ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U16ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U16ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new U16ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U16ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U16ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U16ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U16ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new U16ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class U32Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U32>
  implements
    Indexable<number, AlgebraicTypeVariants.U32>,
    Uniqueable<number, AlgebraicTypeVariants.U32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U32>,
    AutoIncrementable<number, AlgebraicTypeVariants.U32>,
    Defaultable<number, AlgebraicTypeVariants.U32>,
    Nameable<number, AlgebraicTypeVariants.U32>
{
  constructor() {
    super(AlgebraicType.U32);
  }
  index(): U32ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U32ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U32ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U32ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new U32ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U32ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U32ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U32ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U32ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new U32ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class U64Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U64>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U64>,
    Uniqueable<bigint, AlgebraicTypeVariants.U64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U64>,
    Defaultable<bigint, AlgebraicTypeVariants.U64>,
    Nameable<bigint, AlgebraicTypeVariants.U64>
{
  constructor() {
    super(AlgebraicType.U64);
  }
  index(): U64ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U64ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U64ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U64ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new U64ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U64ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U64ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U64ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', bigint>> {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U64ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new U64ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class U128Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U128>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U128>,
    Uniqueable<bigint, AlgebraicTypeVariants.U128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U128>,
    Defaultable<bigint, AlgebraicTypeVariants.U128>,
    Nameable<bigint, AlgebraicTypeVariants.U128>
{
  constructor() {
    super(AlgebraicType.U128);
  }
  index(): U128ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U128ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U128ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U128ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): U128ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U128ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U128ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', bigint>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U128ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new U128ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class U256Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U256>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U256>,
    Uniqueable<bigint, AlgebraicTypeVariants.U256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U256>,
    Defaultable<bigint, AlgebraicTypeVariants.U256>,
    Nameable<bigint, AlgebraicTypeVariants.U256>
{
  constructor() {
    super(AlgebraicType.U256);
  }
  index(): U256ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U256ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U256ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U256ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): U256ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U256ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U256ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', bigint>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U256ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new U256ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class I8Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I8>
  implements
    Indexable<number, AlgebraicTypeVariants.I8>,
    Uniqueable<number, AlgebraicTypeVariants.I8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I8>,
    AutoIncrementable<number, AlgebraicTypeVariants.I8>,
    Defaultable<number, AlgebraicTypeVariants.I8>,
    Nameable<number, AlgebraicTypeVariants.I8>
{
  constructor() {
    super(AlgebraicType.I8);
  }
  index(): I8ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I8ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I8ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I8ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new I8ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I8ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I8ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I8ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I8ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new I8ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class I16Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I16>
  implements
    Indexable<number, AlgebraicTypeVariants.I16>,
    Uniqueable<number, AlgebraicTypeVariants.I16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I16>,
    AutoIncrementable<number, AlgebraicTypeVariants.I16>,
    Defaultable<number, AlgebraicTypeVariants.I16>,
    Nameable<number, AlgebraicTypeVariants.I16>
{
  constructor() {
    super(AlgebraicType.I16);
  }
  index(): I16ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I16ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I16ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I16ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new I16ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I16ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I16ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I16ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I16ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new I16ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class I32Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I32>
  implements
    TypeBuilder<number, AlgebraicTypeVariants.I32>,
    Indexable<number, AlgebraicTypeVariants.I32>,
    Uniqueable<number, AlgebraicTypeVariants.I32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I32>,
    AutoIncrementable<number, AlgebraicTypeVariants.I32>,
    Defaultable<number, AlgebraicTypeVariants.I32>,
    Nameable<number, AlgebraicTypeVariants.I32>
{
  constructor() {
    super(AlgebraicType.I32);
  }
  index(): I32ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I32ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I32ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I32ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new I32ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I32ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I32ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I32ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I32ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new I32ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class I64Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I64>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I64>,
    Uniqueable<bigint, AlgebraicTypeVariants.I64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I64>,
    Defaultable<bigint, AlgebraicTypeVariants.I64>,
    Nameable<bigint, AlgebraicTypeVariants.I64>
{
  constructor() {
    super(AlgebraicType.I64);
  }
  index(): I64ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I64ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I64ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I64ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new I64ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I64ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I64ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I64ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', bigint>> {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I64ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new I64ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class I128Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I128>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I128>,
    Uniqueable<bigint, AlgebraicTypeVariants.I128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I128>,
    Defaultable<bigint, AlgebraicTypeVariants.I128>,
    Nameable<bigint, AlgebraicTypeVariants.I128>
{
  constructor() {
    super(AlgebraicType.I128);
  }
  index(): I128ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I128ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I128ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I128ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): I128ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I128ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I128ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', bigint>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I128ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new I128ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class I256Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I256>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I256>,
    Uniqueable<bigint, AlgebraicTypeVariants.I256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I256>,
    Defaultable<bigint, AlgebraicTypeVariants.I256>,
    Nameable<bigint, AlgebraicTypeVariants.I256>
{
  constructor() {
    super(AlgebraicType.I256);
  }
  index(): I256ColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I256ColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I256ColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I256ColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): I256ColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I256ColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I256ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', bigint>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I256ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new I256ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class F32Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.F32>
  implements
    Defaultable<number, AlgebraicTypeVariants.F32>,
    Nameable<number, AlgebraicTypeVariants.F32>
{
  constructor() {
    super(AlgebraicType.F32);
  }
  default(
    value: number
  ): F32ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new F32ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): F32ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new F32ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class F64Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.F64>
  implements
    Defaultable<number, AlgebraicTypeVariants.F64>,
    Nameable<number, AlgebraicTypeVariants.F64>
{
  constructor() {
    super(AlgebraicType.F64);
  }
  default(
    value: number
  ): F64ColumnBuilder<SetField<DefaultMetadata, 'defaultValue', number>> {
    return new F64ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): F64ColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new F64ColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class BoolBuilder
  extends TypeBuilder<boolean, AlgebraicTypeVariants.Bool>
  implements
    Indexable<boolean, AlgebraicTypeVariants.Bool>,
    Uniqueable<boolean, AlgebraicTypeVariants.Bool>,
    PrimaryKeyable<boolean, AlgebraicTypeVariants.Bool>,
    Defaultable<boolean, AlgebraicTypeVariants.Bool>,
    Nameable<boolean, AlgebraicTypeVariants.Bool>
{
  constructor() {
    super(AlgebraicType.Bool);
  }
  index(): BoolColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): BoolColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): BoolColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): BoolColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): BoolColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: boolean
  ): BoolColumnBuilder<SetField<DefaultMetadata, 'defaultValue', boolean>> {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): BoolColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new BoolColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class StringBuilder
  extends TypeBuilder<string, AlgebraicTypeVariants.String>
  implements
    Indexable<string, AlgebraicTypeVariants.String>,
    Uniqueable<string, AlgebraicTypeVariants.String>,
    PrimaryKeyable<string, AlgebraicTypeVariants.String>,
    Defaultable<string, AlgebraicTypeVariants.String>,
    Nameable<string, AlgebraicTypeVariants.String>
{
  constructor() {
    super(AlgebraicType.String);
  }
  index(): StringColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): StringColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): StringColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): StringColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): StringColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: string
  ): StringColumnBuilder<SetField<DefaultMetadata, 'defaultValue', string>> {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): StringColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new StringColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class ArrayBuilder<Element extends TypeBuilder<any, any>>
  extends TypeBuilder<
    Array<InferTypeOfTypeBuilder<Element>>,
    { tag: 'Array'; value: InferSpacetimeTypeOfTypeBuilder<Element> }
  >
  implements
    Defaultable<Array<InferTypeOfTypeBuilder<Element>>, any>,
    Nameable<Array<InferTypeOfTypeBuilder<Element>>, any>
{
  element: Element;

  constructor(element: Element) {
    super(AlgebraicType.Array(element.algebraicType));
    this.element = element;
  }
  default(
    value: Array<InferTypeOfTypeBuilder<Element>>
  ): ArrayColumnBuilder<
    Element,
    SetField<DefaultMetadata, 'defaultValue', any>
  > {
    return new ArrayColumnBuilder(
      this.element,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ArrayColumnBuilder<Element, SetField<DefaultMetadata, 'name', Name>> {
    return new ArrayColumnBuilder(this.element, set(defaultMetadata, { name }));
  }
}

export class ByteArrayBuilder
  extends TypeBuilder<
    Uint8Array,
    { tag: 'Array'; value: AlgebraicTypeVariants.U8 }
  >
  implements Defaultable<Uint8Array, any>, Nameable<Uint8Array, any>
{
  constructor() {
    super(AlgebraicType.Array(AlgebraicType.U8));
  }
  default(
    value: Uint8Array
  ): ByteArrayColumnBuilder<SetField<DefaultMetadata, 'defaultValue', any>> {
    return new ByteArrayColumnBuilder(
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ByteArrayColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new ByteArrayColumnBuilder(set(defaultMetadata, { name }));
  }
}

export class OptionBuilder<Value extends TypeBuilder<any, any>>
  extends TypeBuilder<
    InferTypeOfTypeBuilder<Value> | undefined,
    OptionAlgebraicType<InferSpacetimeTypeOfTypeBuilder<Value>>
  >
  implements
    Defaultable<
      InferTypeOfTypeBuilder<Value> | undefined,
      OptionAlgebraicType<InferSpacetimeTypeOfTypeBuilder<Value>>
    >,
    Nameable<
      InferTypeOfTypeBuilder<Value> | undefined,
      OptionAlgebraicType<InferSpacetimeTypeOfTypeBuilder<Value>>
    >
{
  value: Value;

  constructor(value: Value) {
    super(Option.getAlgebraicType(value.algebraicType));
    this.value = value;
  }
  default(
    value: InferTypeOfTypeBuilder<Value> | undefined
  ): OptionColumnBuilder<
    Value,
    SetField<
      DefaultMetadata,
      'defaultValue',
      InferTypeOfTypeBuilder<Value> | undefined
    >
  > {
    return new OptionColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): OptionColumnBuilder<Value, SetField<DefaultMetadata, 'name', Name>> {
    return new OptionColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

type ElementsToProductType<Elements extends ElementsObj> = {
  tag: 'Product';
  value: { elements: ElementsArrayFromElementsObj<Elements> };
};

export class ProductBuilder<Elements extends ElementsObj>
  extends TypeBuilder<ObjectType<Elements>, ElementsToProductType<Elements>>
  implements
    Defaultable<ObjectType<Elements>, ElementsToProductType<Elements>>,
    Nameable<ObjectType<Elements>, ElementsToProductType<Elements>>
{
  readonly typeName: string | undefined;
  readonly elements: Elements;
  constructor(elements: Elements, name?: string) {
    function elementsArrayFromElementsObj<Obj extends ElementsObj>(obj: Obj) {
      return Object.keys(obj).map(key => ({
        name: key,
        // Lazily resolve the underlying object's algebraicType.
        // This will call obj[key].algebraicType only when someone
        // actually reads this property.
        get algebraicType() {
          const value = obj[key].algebraicType;
          Object.defineProperty(this, 'algebraicType', { value });
          Object.freeze(this);
          return value;
        },
      }));
    }
    super(
      AlgebraicType.Product({
        elements: elementsArrayFromElementsObj(elements),
      })
    );
    this.typeName = name;
    this.elements = elements;
  }
  default(
    value: ObjectType<Elements>
  ): ProductColumnBuilder<
    Elements,
    SetField<DefaultMetadata, 'defaultValue', any>
  > {
    return new ProductColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ProductColumnBuilder<Elements, SetField<DefaultMetadata, 'name', Name>> {
    return new ProductColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class ResultBuilder<
    Ok extends TypeBuilder<any, any>,
    Err extends TypeBuilder<any, any>,
  >
  extends TypeBuilder<
    InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>,
    ResultAlgebraicType<
      InferSpacetimeTypeOfTypeBuilder<Ok>,
      InferSpacetimeTypeOfTypeBuilder<Err>
    >
  >
  implements
    Defaultable<
      InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>,
      ResultAlgebraicType<
        InferSpacetimeTypeOfTypeBuilder<Ok>,
        InferSpacetimeTypeOfTypeBuilder<Err>
      >
    >
{
  ok: Ok;
  err: Err;

  constructor(ok: Ok, err: Err) {
    super(Result.getAlgebraicType(ok.algebraicType, err.algebraicType));
    this.ok = ok;
    this.err = err;
  }
  default(
    value: InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>
  ): ResultColumnBuilder<
    Ok,
    Err,
    SetField<
      DefaultMetadata,
      'defaultValue',
      InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>
    >
  > {
    return new ResultColumnBuilder<
      Ok,
      Err,
      SetField<
        DefaultMetadata,
        'defaultValue',
        InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>
      >
    >(this, set(defaultMetadata, { defaultValue: value }));
  }
}

class UnitBuilder extends TypeBuilder<
  {},
  { tag: 'Product'; value: { elements: [] } }
> {
  constructor() {
    super({ tag: 'Product', value: { elements: [] } });
  }
}

export class RowBuilder<Row extends RowObj> extends TypeBuilder<
  RowType<CoerceRow<Row>>,
  {
    tag: 'Product';
    value: { elements: ElementsArrayFromRowObj<CoerceRow<Row>> };
  }
> {
  readonly row: CoerceRow<Row>;
  typeName: string | undefined;
  constructor(row: Row, name?: string) {
    const mappedRow = Object.fromEntries(
      Object.entries(row).map(([colName, builder]) => [
        colName,
        builder instanceof ColumnBuilder
          ? builder
          : new ColumnBuilder(builder, {}),
      ])
    ) as CoerceRow<Row>;

    const elements = Object.keys(mappedRow).map(name => ({
      name,
      get algebraicType() {
        const value = mappedRow[name].typeBuilder.algebraicType;
        Object.defineProperty(this, 'algebraicType', { value });
        Object.freeze(this);
        return value;
      },
    }));

    super(AlgebraicType.Product({ elements }));
    this.row = mappedRow;
    this.typeName = name;
  }
}

// Value type produced for a given variant key + builder
type EnumValue<K extends string, B extends TypeBuilder<any, any>> =
  IsUnit<B> extends true
    ? { tag: K }
    : { tag: K; value: InferTypeOfTypeBuilder<B> };

type VariantConstructor<K extends string, V extends TypeBuilder<any, any>> =
  IsUnit<V> extends true
    ? EnumValue<K, V>
    : (value: InferTypeOfTypeBuilder<V>) => EnumValue<K, V>;

type SumBuilderVariantConstructors<Variants extends VariantsObj> = {
  [K in keyof Variants & string]: VariantConstructor<K, Variants[K]>;
};

export type SumBuilder<Variants extends VariantsObj> =
  SumBuilderImpl<Variants> & SumBuilderVariantConstructors<Variants>;

type VariantsToSumType<Variants extends VariantsObj> = {
  tag: 'Sum';
  value: { variants: VariantsArrayFromVariantsObj<Variants> };
};

class SumBuilderImpl<Variants extends VariantsObj>
  extends TypeBuilder<EnumType<Variants>, VariantsToSumType<Variants>>
  implements
    Defaultable<EnumType<Variants>, VariantsToSumType<Variants>>,
    Nameable<EnumType<Variants>, VariantsToSumType<Variants>>
{
  readonly variants: Variants;
  readonly typeName: string | undefined;

  constructor(variants: Variants, name?: string) {
    function variantsArrayFromVariantsObj<Variants extends VariantsObj>(
      variants: Variants
    ) {
      return (Object.keys(variants) as Array<keyof Variants>).map(key => ({
        name: key as string,
        // Lazily resolve the underlying object's algebraicType.
        // This will call obj[key].algebraicType only when someone
        // actually reads this property.
        get algebraicType() {
          const value = variants[key].algebraicType;
          Object.defineProperty(this, 'algebraicType', { value });
          Object.freeze(this);
          return value;
        },
      }));
    }
    super(
      AlgebraicType.Sum({
        variants: variantsArrayFromVariantsObj(variants),
      })
    );

    this.variants = variants;
    this.typeName = name;

    for (const key of Object.keys(variants) as Array<keyof Variants & string>) {
      const desc = Object.getOwnPropertyDescriptor(variants, key);

      const isAccessor =
        !!desc &&
        (typeof desc.get === 'function' || typeof desc.set === 'function');

      let isUnit = false;

      if (!isAccessor) {
        // Only read variants[key] if it's a *data* property
        // otherwise assume non-unit because it's a getter
        const variant = variants[key];
        isUnit = variant instanceof UnitBuilder;
      }

      if (isUnit) {
        // Unit: expose a read-only VALUE (no call)
        const constant = this.create(key as any) as EnumValue<
          typeof key,
          Variants[typeof key]
        >;
        Object.defineProperty(this, key, {
          value: constant,
          writable: false,
          enumerable: true,
          configurable: false,
        });
      } else {
        const fn = ((value: any) =>
          this.create(key as any, value)) as VariantConstructor<
          typeof key & string,
          Variants[typeof key]
        >;

        Object.defineProperty(this, key, {
          value: fn,
          writable: false,
          enumerable: true,
          configurable: false,
        });
      }
    }
  }

  /**
   * Create a value of this sum type.
   * - Unit variants: create('bar')
   * - Payload variants: create('foo', value)
   */
  private create<K extends keyof Variants & string>(
    tag: K
  ): EnumValue<K, Variants[K]>;
  private create<K extends keyof Variants & string>(
    tag: K,
    value: InferTypeOfTypeBuilder<Variants[K]>
  ): EnumValue<K, Variants[K]>;
  private create(tag: string, value?: unknown) {
    return value === undefined ? { tag } : { tag, value };
  }

  default(
    value: EnumType<Variants>
  ): SumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'defaultValue', any>
  > {
    return new SumColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): SumColumnBuilder<Variants, SetField<DefaultMetadata, 'name', Name>> {
    return new SumColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export const SumBuilder: {
  new <Variants extends VariantsObj>(
    variants: Variants,
    name?: string
  ): SumBuilder<Variants>;
  [Symbol.hasInstance](x: any): x is SumBuilder<VariantsObj>;
} = SumBuilderImpl as any;

class SimpleSumBuilderImpl<Variants extends SimpleVariantsObj>
  extends SumBuilderImpl<Variants>
  implements
    Indexable<
      EnumType<Variants>,
      {
        tag: 'Sum';
        value: { variants: VariantsArrayFromVariantsObj<Variants> };
      }
    >,
    PrimaryKeyable<
      EnumType<Variants>,
      {
        tag: 'Sum';
        value: { variants: VariantsArrayFromVariantsObj<Variants> };
      }
    >
{
  index(): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'indexType', N>
  >;
  index(
    algorithm: IndexTypes = 'btree'
  ): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'indexType', IndexTypes>
  > {
    return new SimpleSumColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  primaryKey(): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new SimpleSumColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
}

export const SimpleSumBuilder: {
  new <Variants extends SimpleVariantsObj>(
    variants: Variants,
    name?: string
  ): SimpleSumBuilderImpl<Variants> & SumBuilderVariantConstructors<Variants>;
} = SimpleSumBuilderImpl as any;

export type SimpleSumBuilder<Variants extends SimpleVariantsObj> =
  SimpleSumBuilderImpl<Variants> & SumBuilderVariantConstructors<Variants>;

export class ScheduleAtBuilder
  extends TypeBuilder<ScheduleAt, ScheduleAtAlgebraicType>
  implements
    Defaultable<ScheduleAt, ScheduleAtAlgebraicType>,
    Nameable<ScheduleAt, ScheduleAtAlgebraicType>
{
  constructor() {
    super(ScheduleAt.getAlgebraicType());
  }
  default(
    value: ScheduleAt
  ): ScheduleAtColumnBuilder<
    SetField<DefaultMetadata, 'defaultValue', ScheduleAt>
  > {
    return new ScheduleAtColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ScheduleAtColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new ScheduleAtColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class IdentityBuilder
  extends TypeBuilder<Identity, IdentityAlgebraicType>
  implements
    Indexable<Identity, IdentityAlgebraicType>,
    Uniqueable<Identity, IdentityAlgebraicType>,
    PrimaryKeyable<Identity, IdentityAlgebraicType>,
    Defaultable<Identity, IdentityAlgebraicType>,
    Nameable<Identity, IdentityAlgebraicType>
{
  constructor() {
    super(Identity.getAlgebraicType());
  }
  index(): IdentityColumnBuilder<
    SetField<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): IdentityColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): IdentityColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): IdentityColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): IdentityColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): IdentityColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: Identity
  ): IdentityColumnBuilder<
    SetField<DefaultMetadata, 'defaultValue', Identity>
  > {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): IdentityColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new IdentityColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class ConnectionIdBuilder
  extends TypeBuilder<ConnectionId, ConnectionIdAlgebraicType>
  implements
    Indexable<ConnectionId, ConnectionIdAlgebraicType>,
    Uniqueable<ConnectionId, ConnectionIdAlgebraicType>,
    PrimaryKeyable<ConnectionId, ConnectionIdAlgebraicType>,
    Defaultable<ConnectionId, ConnectionIdAlgebraicType>,
    Nameable<ConnectionId, ConnectionIdAlgebraicType>
{
  constructor() {
    super(ConnectionId.getAlgebraicType());
  }
  index(): ConnectionIdColumnBuilder<
    SetField<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): ConnectionIdColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): ConnectionIdColumnBuilder<
    SetField<DefaultMetadata, 'indexType', IndexTypes>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): ConnectionIdColumnBuilder<
    SetField<DefaultMetadata, 'isUnique', true>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): ConnectionIdColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): ConnectionIdColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: ConnectionId
  ): ConnectionIdColumnBuilder<
    SetField<DefaultMetadata, 'defaultValue', ConnectionId>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ConnectionIdColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new ConnectionIdColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class TimestampBuilder
  extends TypeBuilder<Timestamp, TimestampAlgebraicType>
  implements
    Indexable<Timestamp, TimestampAlgebraicType>,
    Uniqueable<Timestamp, TimestampAlgebraicType>,
    PrimaryKeyable<Timestamp, TimestampAlgebraicType>,
    Defaultable<Timestamp, TimestampAlgebraicType>,
    Nameable<Timestamp, TimestampAlgebraicType>
{
  constructor() {
    super(Timestamp.getAlgebraicType());
  }
  index(): TimestampColumnBuilder<
    SetField<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimestampColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimestampColumnBuilder<
    SetField<DefaultMetadata, 'indexType', IndexTypes>
  > {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): TimestampColumnBuilder<
    SetField<DefaultMetadata, 'isUnique', true>
  > {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimestampColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): TimestampColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: Timestamp
  ): TimestampColumnBuilder<
    SetField<DefaultMetadata, 'defaultValue', Timestamp>
  > {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): TimestampColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new TimestampColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class TimeDurationBuilder
  extends TypeBuilder<TimeDuration, TimeDurationAlgebraicType>
  implements
    Indexable<TimeDuration, TimeDurationAlgebraicType>,
    Uniqueable<TimeDuration, TimeDurationAlgebraicType>,
    PrimaryKeyable<TimeDuration, TimeDurationAlgebraicType>,
    Defaultable<TimeDuration, TimeDurationAlgebraicType>,
    Nameable<TimeDuration, TimeDurationAlgebraicType>
{
  constructor() {
    super(TimeDuration.getAlgebraicType());
  }
  index(): TimeDurationColumnBuilder<
    SetField<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimeDurationColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimeDurationColumnBuilder<
    SetField<DefaultMetadata, 'indexType', IndexTypes>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): TimeDurationColumnBuilder<
    SetField<DefaultMetadata, 'isUnique', true>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimeDurationColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): TimeDurationColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: TimeDuration
  ): TimeDurationColumnBuilder<
    SetField<DefaultMetadata, 'defaultValue', TimeDuration>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): TimeDurationColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new TimeDurationColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

export class UuidBuilder
  extends TypeBuilder<Uuid, UuidAlgebraicType>
  implements
    Indexable<Uuid, UuidAlgebraicType>,
    Uniqueable<Uuid, UuidAlgebraicType>,
    PrimaryKeyable<Uuid, UuidAlgebraicType>,
    Defaultable<Uuid, UuidAlgebraicType>,
    Nameable<Uuid, UuidAlgebraicType>
{
  constructor() {
    super(Uuid.getAlgebraicType());
  }
  index(): UuidColumnBuilder<SetField<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): UuidColumnBuilder<SetField<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): UuidColumnBuilder<SetField<DefaultMetadata, 'indexType', IndexTypes>> {
    return new UuidColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): UuidColumnBuilder<SetField<DefaultMetadata, 'isUnique', true>> {
    return new UuidColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): UuidColumnBuilder<
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new UuidColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): UuidColumnBuilder<
    SetField<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new UuidColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: Uuid
  ): UuidColumnBuilder<SetField<DefaultMetadata, 'defaultValue', Uuid>> {
    return new UuidColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): UuidColumnBuilder<SetField<DefaultMetadata, 'name', Name>> {
    return new UuidColumnBuilder(this, set(defaultMetadata, { name }));
  }
}

/**
 * The type of index types that can be applied to a column.
 * `undefined` is the default
 */
export type IndexTypes = 'btree' | 'direct' | undefined;

/**
 * Metadata describing column constraints and index type
 */
export type ColumnMetadata<Type = any> = {
  isPrimaryKey?: true;
  isUnique?: true;
  isAutoIncrement?: true;
  indexType?: IndexTypes;
  defaultValue?: Type;
  name?: string;
};

/**
 * Default metadata state type for a newly created column
 */
type DefaultMetadata = object;

/**
 * Default metadata state value for a newly created column
 */
const defaultMetadata: ColumnMetadata<never> = {};

/**
 * A column builder allows you to incrementally specify constraints
 * and metadata for a column in a type-safe way.
 *
 * It carries both a phantom TypeScript type (the `Type`) and
 * runtime algebraic type information.
 *
 * IMPORTANT! We have deliberately chosen to not have {@link ColumnBuilder}
 * extend {@link TypeBuilder} so that you cannot pass a {@link ColumnBuilder}
 * where a {@link TypeBuilder} is expected. i.e. We want to maintain
 * contravariance for functions that accept {@link TypeBuilder} parameters.
 */
export class ColumnBuilder<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  typeBuilder: TypeBuilder<Type, SpacetimeType>;
  columnMetadata: M;

  constructor(typeBuilder: TypeBuilder<Type, SpacetimeType>, metadata: M) {
    this.typeBuilder = typeBuilder;
    this.columnMetadata = metadata;
  }

  serialize(writer: BinaryWriter, value: Type): void {
    this.typeBuilder.serialize(writer, value);
  }

  deserialize(reader: BinaryReader): Type {
    return this.typeBuilder.deserialize(reader);
  }
}

export class U8ColumnBuilder<M extends ColumnMetadata<number> = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.U8, M>
  implements
    Indexable<number, AlgebraicTypeVariants.U8>,
    Uniqueable<number, AlgebraicTypeVariants.U8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U8>,
    AutoIncrementable<number, AlgebraicTypeVariants.U8>,
    Defaultable<number, AlgebraicTypeVariants.U8>,
    Nameable<number, AlgebraicTypeVariants.U8>
{
  index(): U8ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U8ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U8ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U8ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U8ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U8ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true as const })
    );
  }
  default(value: number): U8ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U8ColumnBuilder<SetField<M, 'name', Name>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class U16ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.U16, M>
  implements
    Indexable<number, AlgebraicTypeVariants.U16>,
    Uniqueable<number, AlgebraicTypeVariants.U16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U16>,
    AutoIncrementable<number, AlgebraicTypeVariants.U16>,
    Defaultable<number, AlgebraicTypeVariants.U16>,
    Nameable<number, AlgebraicTypeVariants.U16>
{
  index(): U16ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U16ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U16ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U16ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U16ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U16ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U16ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U16ColumnBuilder<SetField<M, 'name', Name>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class U32ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.U32, M>
  implements
    Indexable<number, AlgebraicTypeVariants.U32>,
    Uniqueable<number, AlgebraicTypeVariants.U32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U32>,
    AutoIncrementable<number, AlgebraicTypeVariants.U32>,
    Defaultable<number, AlgebraicTypeVariants.U32>,
    Nameable<number, AlgebraicTypeVariants.U32>
{
  index(): U32ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U32ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U32ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U32ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U32ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U32ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U32ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U32ColumnBuilder<SetField<M, 'name', Name>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class U64ColumnBuilder<
    M extends ColumnMetadata<bigint> = DefaultMetadata,
  >
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.U64, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U64>,
    Uniqueable<bigint, AlgebraicTypeVariants.U64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U64>,
    Defaultable<bigint, AlgebraicTypeVariants.U64>,
    Nameable<bigint, AlgebraicTypeVariants.U64>
{
  index(): U64ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U64ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U64ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U64ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U64ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U64ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U64ColumnBuilder<SetField<M, 'defaultValue', bigint>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U64ColumnBuilder<SetField<M, 'name', Name>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class U128ColumnBuilder<
    M extends ColumnMetadata<bigint> = DefaultMetadata,
  >
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.U128, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U128>,
    Uniqueable<bigint, AlgebraicTypeVariants.U128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U128>,
    Defaultable<bigint, AlgebraicTypeVariants.U128>,
    Nameable<bigint, AlgebraicTypeVariants.U128>
{
  index(): U128ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U128ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U128ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U128ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U128ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U128ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U128ColumnBuilder<SetField<M, 'defaultValue', bigint>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U128ColumnBuilder<SetField<M, 'name', Name>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class U256ColumnBuilder<
    M extends ColumnMetadata<bigint> = DefaultMetadata,
  >
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.U256, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U256>,
    Uniqueable<bigint, AlgebraicTypeVariants.U256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U256>,
    Defaultable<bigint, AlgebraicTypeVariants.U256>,
    Nameable<bigint, AlgebraicTypeVariants.U256>
{
  index(): U256ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U256ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U256ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U256ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U256ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U256ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U256ColumnBuilder<SetField<M, 'defaultValue', bigint>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): U256ColumnBuilder<SetField<M, 'name', Name>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class I8ColumnBuilder<M extends ColumnMetadata<number> = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.I8, M>
  implements
    Indexable<number, AlgebraicTypeVariants.I8>,
    Uniqueable<number, AlgebraicTypeVariants.I8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I8>,
    AutoIncrementable<number, AlgebraicTypeVariants.I8>,
    Defaultable<number, AlgebraicTypeVariants.I8>,
    Nameable<number, AlgebraicTypeVariants.I8>
{
  index(): I8ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I8ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I8ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I8ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I8ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I8ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: number): I8ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I8ColumnBuilder<SetField<M, 'name', Name>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class I16ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.I16, M>
  implements
    Indexable<number, AlgebraicTypeVariants.I16>,
    Uniqueable<number, AlgebraicTypeVariants.I16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I16>,
    AutoIncrementable<number, AlgebraicTypeVariants.I16>,
    Defaultable<number, AlgebraicTypeVariants.I16>,
    Nameable<number, AlgebraicTypeVariants.I16>
{
  index(): I16ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I16ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I16ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I16ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I16ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I16ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I16ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I16ColumnBuilder<SetField<M, 'name', Name>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class I32ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.I32, M>
  implements
    Indexable<number, AlgebraicTypeVariants.I32>,
    Uniqueable<number, AlgebraicTypeVariants.I32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I32>,
    AutoIncrementable<number, AlgebraicTypeVariants.I32>,
    Defaultable<number, AlgebraicTypeVariants.I32>,
    Nameable<number, AlgebraicTypeVariants.I32>
{
  index(): I32ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I32ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I32ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I32ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I32ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I32ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I32ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I32ColumnBuilder<SetField<M, 'name', Name>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class I64ColumnBuilder<
    M extends ColumnMetadata<bigint> = DefaultMetadata,
  >
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.I64, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I64>,
    Uniqueable<bigint, AlgebraicTypeVariants.I64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I64>,
    Defaultable<bigint, AlgebraicTypeVariants.I64>,
    Nameable<bigint, AlgebraicTypeVariants.I64>
{
  index(): I64ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I64ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I64ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I64ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I64ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I64ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I64ColumnBuilder<SetField<M, 'defaultValue', bigint>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I64ColumnBuilder<SetField<M, 'name', Name>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class I128ColumnBuilder<
    M extends ColumnMetadata<bigint> = DefaultMetadata,
  >
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.I128, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I128>,
    Uniqueable<bigint, AlgebraicTypeVariants.I128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I128>,
    Defaultable<bigint, AlgebraicTypeVariants.I128>,
    Nameable<bigint, AlgebraicTypeVariants.I128>
{
  index(): I128ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I128ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I128ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I128ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I128ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I128ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I128ColumnBuilder<SetField<M, 'defaultValue', bigint>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I128ColumnBuilder<SetField<M, 'name', Name>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class I256ColumnBuilder<
    M extends ColumnMetadata<bigint> = DefaultMetadata,
  >
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.I256, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I256>,
    Uniqueable<bigint, AlgebraicTypeVariants.I256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I256>,
    Defaultable<bigint, AlgebraicTypeVariants.I256>,
    Nameable<bigint, AlgebraicTypeVariants.I256>
{
  index(): I256ColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I256ColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I256ColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I256ColumnBuilder<SetField<M, 'isUnique', true>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I256ColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I256ColumnBuilder<SetField<M, 'isAutoIncrement', true>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I256ColumnBuilder<SetField<M, 'defaultValue', bigint>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): I256ColumnBuilder<SetField<M, 'name', Name>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class F32ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.F32, M>
  implements
    Defaultable<number, AlgebraicTypeVariants.F32>,
    Nameable<number, AlgebraicTypeVariants.F32>
{
  default(
    value: number
  ): F32ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new F32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): F32ColumnBuilder<SetField<M, 'name', Name>> {
    return new F32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class F64ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.F64, M>
  implements
    Defaultable<number, AlgebraicTypeVariants.F64>,
    Nameable<number, AlgebraicTypeVariants.F64>
{
  default(
    value: number
  ): F64ColumnBuilder<SetField<M, 'defaultValue', number>> {
    return new F64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): F64ColumnBuilder<SetField<M, 'name', Name>> {
    return new F64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class BoolColumnBuilder<
    M extends ColumnMetadata<boolean> = DefaultMetadata,
  >
  extends ColumnBuilder<boolean, AlgebraicTypeVariants.Bool, M>
  implements
    Indexable<boolean, AlgebraicTypeVariants.Bool>,
    Uniqueable<boolean, AlgebraicTypeVariants.Bool>,
    PrimaryKeyable<boolean, AlgebraicTypeVariants.Bool>,
    Defaultable<boolean, AlgebraicTypeVariants.Bool>,
    Nameable<boolean, AlgebraicTypeVariants.Bool>
{
  index(): BoolColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): BoolColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): BoolColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): BoolColumnBuilder<SetField<M, 'isUnique', true>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): BoolColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: boolean
  ): BoolColumnBuilder<SetField<M, 'defaultValue', boolean>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): BoolColumnBuilder<SetField<M, 'name', Name>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class StringColumnBuilder<
    M extends ColumnMetadata<string> = DefaultMetadata,
  >
  extends ColumnBuilder<string, AlgebraicTypeVariants.String, M>
  implements
    Indexable<string, AlgebraicTypeVariants.String>,
    Uniqueable<string, AlgebraicTypeVariants.String>,
    PrimaryKeyable<string, AlgebraicTypeVariants.String>,
    Defaultable<string, AlgebraicTypeVariants.String>,
    Nameable<string, AlgebraicTypeVariants.String>
{
  index(): StringColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): StringColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): StringColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): StringColumnBuilder<SetField<M, 'isUnique', true>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): StringColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: string
  ): StringColumnBuilder<SetField<M, 'defaultValue', string>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): StringColumnBuilder<SetField<M, 'name', Name>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class ArrayColumnBuilder<
    Element extends TypeBuilder<any, any>,
    M extends ColumnMetadata<
      Array<InferTypeOfTypeBuilder<Element>>
    > = DefaultMetadata,
  >
  extends ColumnBuilder<
    Array<InferTypeOfTypeBuilder<Element>>,
    { tag: 'Array'; value: InferSpacetimeTypeOfTypeBuilder<Element> },
    M
  >
  implements
    Defaultable<
      Array<InferTypeOfTypeBuilder<Element>>,
      AlgebraicTypeVariants.Array
    >,
    Nameable<
      Array<InferTypeOfTypeBuilder<Element>>,
      AlgebraicTypeVariants.Array
    >
{
  default(
    value: Array<InferTypeOfTypeBuilder<Element>>
  ): ArrayColumnBuilder<
    Element,
    SetField<M, 'defaultValue', Array<InferTypeOfTypeBuilder<Element>>>
  > {
    return new ArrayColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ArrayColumnBuilder<Element, SetField<M, 'name', Name>> {
    return new ArrayColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

type ByteArrayType = {
  tag: 'Array';
  value: AlgebraicTypeVariants.U8;
};

export class ByteArrayColumnBuilder<
    M extends ColumnMetadata<Uint8Array> = DefaultMetadata,
  >
  extends ColumnBuilder<Uint8Array, ByteArrayType, M>
  implements
    Defaultable<Uint8Array, ByteArrayType, M>,
    Nameable<Uint8Array, ByteArrayType, M>
{
  constructor(metadata: M) {
    super(new TypeBuilder(AlgebraicType.Array(AlgebraicType.U8)), metadata);
  }
  default(
    value: Uint8Array
  ): ByteArrayColumnBuilder<SetField<M, 'defaultValue', Uint8Array>> {
    return new ByteArrayColumnBuilder(
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ByteArrayColumnBuilder<SetField<M, 'name', Name>> {
    return new ByteArrayColumnBuilder(set(this.columnMetadata, { name }));
  }
}

export class OptionColumnBuilder<
    Value extends TypeBuilder<any, any>,
    M extends ColumnMetadata<
      InferTypeOfTypeBuilder<Value> | undefined
    > = DefaultMetadata,
  >
  extends ColumnBuilder<
    InferTypeOfTypeBuilder<Value> | undefined,
    OptionAlgebraicType<InferSpacetimeTypeOfTypeBuilder<Value>>,
    M
  >
  implements
    Defaultable<
      InferTypeOfTypeBuilder<Value> | undefined,
      OptionAlgebraicType<InferSpacetimeTypeOfTypeBuilder<Value>>
    >,
    Nameable<
      InferTypeOfTypeBuilder<Value> | undefined,
      OptionAlgebraicType<InferSpacetimeTypeOfTypeBuilder<Value>>
    >
{
  default(
    value: InferTypeOfTypeBuilder<Value> | undefined
  ): OptionColumnBuilder<
    Value,
    SetField<M, 'defaultValue', InferTypeOfTypeBuilder<Value> | undefined>
  > {
    return new OptionColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
  name<const Name extends string>(
    name: Name
  ): OptionColumnBuilder<Value, SetField<M, 'name', Name>> {
    return new OptionColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class ResultColumnBuilder<
    Ok extends TypeBuilder<any, any>,
    Err extends TypeBuilder<any, any>,
    M extends ColumnMetadata<
      InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>
    > = DefaultMetadata,
  >
  extends ColumnBuilder<
    InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>,
    ResultAlgebraicType<
      InferSpacetimeTypeOfTypeBuilder<Ok>,
      InferSpacetimeTypeOfTypeBuilder<Err>
    >,
    M
  >
  implements
    Defaultable<
      InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>,
      ResultAlgebraicType<
        InferSpacetimeTypeOfTypeBuilder<Ok>,
        InferSpacetimeTypeOfTypeBuilder<Err>
      >
    >
{
  constructor(typeBuilder: TypeBuilder<any, any>, metadata: M) {
    super(typeBuilder, metadata);
  }

  default(
    value: InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>
  ): ResultColumnBuilder<
    Ok,
    Err,
    SetField<
      M,
      'defaultValue',
      InferTypeOfTypeBuilder<Ok> | InferTypeOfTypeBuilder<Err>
    >
  > {
    return new ResultColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, {
        defaultValue: value,
      })
    );
  }
}

export class ProductColumnBuilder<
    Elements extends ElementsObj,
    M extends ColumnMetadata<ObjectType<Elements>> = DefaultMetadata,
  >
  extends ColumnBuilder<
    ObjectType<Elements>,
    ElementsToProductType<Elements>,
    M
  >
  implements
    Defaultable<ObjectType<Elements>, ElementsToProductType<Elements>>,
    Nameable<ObjectType<Elements>, ElementsToProductType<Elements>>
{
  default(
    value: ObjectType<Elements>
  ): ProductColumnBuilder<
    Elements,
    SetField<DefaultMetadata, 'defaultValue', any>
  > {
    return new ProductColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ProductColumnBuilder<Elements, SetField<DefaultMetadata, 'name', Name>> {
    return new ProductColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class SumColumnBuilder<
    Variants extends VariantsObj,
    M extends ColumnMetadata<EnumType<Variants>> = DefaultMetadata,
  >
  extends ColumnBuilder<EnumType<Variants>, VariantsToSumType<Variants>, M>
  implements
    Defaultable<EnumType<Variants>, VariantsToSumType<Variants>>,
    Nameable<EnumType<Variants>, VariantsToSumType<Variants>>
{
  default(
    value: EnumType<Variants>
  ): SumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'defaultValue', any>
  > {
    return new SumColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): SumColumnBuilder<Variants, SetField<DefaultMetadata, 'name', Name>> {
    return new SumColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class SimpleSumColumnBuilder<
    Variants extends VariantsObj,
    M extends ColumnMetadata<EnumType<Variants>> = DefaultMetadata,
  >
  extends SumColumnBuilder<Variants, M>
  implements
    Indexable<EnumType<Variants>, AlgebraicTypeVariants.Sum>,
    PrimaryKeyable<EnumType<Variants>, AlgebraicTypeVariants.Sum>
{
  index(): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'indexType', N>
  >;
  index(
    algorithm: IndexTypes = 'btree'
  ): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'indexType', IndexTypes>
  > {
    return new SimpleSumColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  primaryKey(): SimpleSumColumnBuilder<
    Variants,
    SetField<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new SimpleSumColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
}

export class ScheduleAtColumnBuilder<
    M extends ColumnMetadata<ScheduleAt> = DefaultMetadata,
  >
  extends ColumnBuilder<ScheduleAt, ScheduleAtAlgebraicType, M>
  implements
    Defaultable<ScheduleAt, ScheduleAtAlgebraicType>,
    Nameable<ScheduleAt, ScheduleAtAlgebraicType>
{
  default(
    value: ScheduleAt
  ): ScheduleAtColumnBuilder<SetField<M, 'defaultValue', ScheduleAt>> {
    return new ScheduleAtColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ScheduleAtColumnBuilder<SetField<M, 'name', Name>> {
    return new ScheduleAtColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class IdentityColumnBuilder<
    M extends ColumnMetadata<Identity> = DefaultMetadata,
  >
  extends ColumnBuilder<Identity, IdentityAlgebraicType, M>
  implements
    Indexable<Identity, IdentityAlgebraicType>,
    Uniqueable<Identity, IdentityAlgebraicType>,
    PrimaryKeyable<Identity, IdentityAlgebraicType>,
    Defaultable<Identity, IdentityAlgebraicType>,
    Nameable<Identity, IdentityAlgebraicType>
{
  index(): IdentityColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): IdentityColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): IdentityColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): IdentityColumnBuilder<SetField<M, 'isUnique', true>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): IdentityColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: Identity
  ): IdentityColumnBuilder<SetField<M, 'defaultValue', Identity>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): IdentityColumnBuilder<SetField<M, 'name', Name>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class ConnectionIdColumnBuilder<
    M extends ColumnMetadata<ConnectionId> = DefaultMetadata,
  >
  extends ColumnBuilder<ConnectionId, ConnectionIdAlgebraicType, M>
  implements
    Indexable<ConnectionId, ConnectionIdAlgebraicType>,
    Uniqueable<ConnectionId, ConnectionIdAlgebraicType>,
    PrimaryKeyable<ConnectionId, ConnectionIdAlgebraicType>,
    Defaultable<ConnectionId, ConnectionIdAlgebraicType>,
    Nameable<ConnectionId, ConnectionIdAlgebraicType>
{
  index(): ConnectionIdColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): ConnectionIdColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): ConnectionIdColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): ConnectionIdColumnBuilder<SetField<M, 'isUnique', true>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): ConnectionIdColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: ConnectionId
  ): ConnectionIdColumnBuilder<SetField<M, 'defaultValue', ConnectionId>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): ConnectionIdColumnBuilder<SetField<M, 'name', Name>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class TimestampColumnBuilder<
    M extends ColumnMetadata<Timestamp> = DefaultMetadata,
  >
  extends ColumnBuilder<Timestamp, TimestampAlgebraicType, M>
  implements
    Indexable<Timestamp, TimestampAlgebraicType>,
    Uniqueable<Timestamp, TimestampAlgebraicType>,
    PrimaryKeyable<Timestamp, TimestampAlgebraicType>,
    Defaultable<Timestamp, TimestampAlgebraicType>,
    Nameable<Timestamp, TimestampAlgebraicType>
{
  index(): TimestampColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimestampColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimestampColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): TimestampColumnBuilder<SetField<M, 'isUnique', true>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimestampColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: Timestamp
  ): TimestampColumnBuilder<SetField<M, 'defaultValue', Timestamp>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): TimestampColumnBuilder<SetField<M, 'name', Name>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class TimeDurationColumnBuilder<
    M extends ColumnMetadata<TimeDuration> = DefaultMetadata,
  >
  extends ColumnBuilder<TimeDuration, TimeDurationAlgebraicType, M>
  implements
    Indexable<TimeDuration, TimeDurationAlgebraicType>,
    Uniqueable<TimeDuration, TimeDurationAlgebraicType>,
    PrimaryKeyable<TimeDuration, TimeDurationAlgebraicType>,
    Defaultable<TimeDuration, TimeDurationAlgebraicType>,
    Nameable<TimeDuration, TimeDurationAlgebraicType>
{
  index(): TimeDurationColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimeDurationColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimeDurationColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): TimeDurationColumnBuilder<SetField<M, 'isUnique', true>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimeDurationColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: TimeDuration
  ): TimeDurationColumnBuilder<SetField<M, 'defaultValue', TimeDuration>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): TimeDurationColumnBuilder<SetField<M, 'name', Name>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class UuidColumnBuilder<M extends ColumnMetadata<Uuid> = DefaultMetadata>
  extends ColumnBuilder<Uuid, UuidAlgebraicType, M>
  implements
    Indexable<Uuid, UuidAlgebraicType>,
    Uniqueable<Uuid, UuidAlgebraicType>,
    PrimaryKeyable<Uuid, UuidAlgebraicType>,
    Defaultable<Uuid, UuidAlgebraicType>,
    Nameable<Uuid, UuidAlgebraicType>
{
  index(): UuidColumnBuilder<SetField<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): UuidColumnBuilder<SetField<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): UuidColumnBuilder<SetField<M, 'indexType', IndexTypes>> {
    return new UuidColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): UuidColumnBuilder<SetField<M, 'isUnique', true>> {
    return new UuidColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): UuidColumnBuilder<SetField<M, 'isPrimaryKey', true>> {
    return new UuidColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(value: Uuid): UuidColumnBuilder<SetField<M, 'defaultValue', Uuid>> {
    return new UuidColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
  name<const Name extends string>(
    name: Name
  ): UuidColumnBuilder<SetField<M, 'name', Name>> {
    return new UuidColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { name })
    );
  }
}

export class RefBuilder<Type, SpacetimeType> extends TypeBuilder<
  Type,
  AlgebraicTypeVariants.Ref
> {
  readonly ref: number;
  /** The phantom type of the pointee of this ref. */
  private readonly __spacetimeType!: SpacetimeType;
  constructor(ref: number) {
    super(AlgebraicType.Ref(ref));
    this.ref = ref;
  }
}

interface EnumFn {
  /**
   * Creates a simple sum type whose cases are all unit variants.
   * Each string in the array becomes a case of the enum.
   *
   * Example:
   * ```ts
   * t.enum("Color", ["red", "green", "blue"]);
   * ```
   */
  <Case extends string>(
    name: string,
    cases: readonly [Case, ...Case[]]
  ): SimpleSumBuilderImpl<Record<Case, UnitBuilder>>;

  /**
   * Creates an empty simple sum type (no cases, equivalent to `never`).
   * This can be useful for code generation or placeholder types.
   * Example:
   * ```ts
   * t.enum("Never", []);
   * ```
   */
  (name: string, cases: []): SimpleSumBuilderImpl<Record<never, UnitBuilder>>;

  /**
   * Creates a full sum type, where each case can have a payload.
   * Each value in the object must be a {@link TypeBuilder}.
   *
   * Example:
   * ```ts
   * t.enum("Result", { Ok: t.unit(), Err: t.string() });
   * ```
   */
  <Obj extends VariantsObj>(name: string, obj: Obj): SumBuilder<Obj>;
}

const enumImpl = ((nameOrObj: any, maybeObj?: any) => {
  let obj: any = nameOrObj;
  let name: string | undefined = undefined;

  if (typeof nameOrObj === 'string') {
    if (!maybeObj) {
      throw new TypeError(
        'When providing a name, you must also provide the variants object or array.'
      );
    }
    obj = maybeObj;
    name = nameOrObj;
  }

  // Simple sum (array form)
  if (Array.isArray(obj)) {
    const simpleVariantsObj: Record<string, UnitBuilder> = {};
    for (const variant of obj) {
      simpleVariantsObj[variant] = new UnitBuilder();
    }
    return new SimpleSumBuilderImpl(simpleVariantsObj, name);
  }

  // Regular sum (object form)
  return new SumBuilder(obj, name);
}) as EnumFn;

/**
 * A collection of factory functions for creating various SpacetimeDB algebraic types
 * to be used in table definitions. Each function returns a corresponding builder
 * for a specific type, such as `BoolBuilder`, `StringBuilder`, or `F64Builder`.
 *
 * These builders are used to define the schema of tables in SpacetimeDB, and each
 * builder implements the {@link TypeBuilder} interface, allowing for type-safe
 * schema construction in TypeScript.
 *
 * @remarks
 * - Primitive types (e.g., `bool`, `string`, `number`) map to their respective TypeScript types.
 * - Integer and floating-point types (e.g., `i8`, `u64`, `f32`) are represented as `number` or `bigint` in TypeScript.
 * - Complex types such as `object`, `array`, and `enum` allow for nested and structured schemas.
 * - The `scheduleAt` builder is a special column type for scheduling.
 *
 * @see {@link TypeBuilder}
 */
export const t = {
  /**
   * Creates a new `Bool` {@link AlgebraicType} to be used in table definitions
   * Represented as `boolean` in TypeScript.
   * @returns A new {@link BoolBuilder} instance
   */
  bool: (): BoolBuilder => new BoolBuilder(),

  /**
   * Creates a new `String` {@link AlgebraicType} to be used in table definitions
   * Represented as `string` in TypeScript.
   * @returns A new {@link StringBuilder} instance
   */
  string: (): StringBuilder => new StringBuilder(),

  /**
   * Creates a new `F64` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link F64Builder} instance
   */
  number: (): F64Builder => new F64Builder(),

  /**
   * Creates a new `I8` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link I8Builder} instance
   */
  i8: (): I8Builder => new I8Builder(),

  /**
   * Creates a new `U8` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link U8Builder} instance
   */
  u8: (): U8Builder => new U8Builder(),

  /**
   * Creates a new `I16` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link I16Builder} instance
   */
  i16: (): I16Builder => new I16Builder(),

  /**
   * Creates a new `U16` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link U16Builder} instance
   */
  u16: (): U16Builder => new U16Builder(),

  /**
   * Creates a new `I32` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link I32Builder} instance
   */
  i32: (): I32Builder => new I32Builder(),

  /**
   * Creates a new `U32` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link U32Builder} instance
   */
  u32: (): U32Builder => new U32Builder(),

  /**
   * Creates a new `I64` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new {@link I64Builder} instance
   */
  i64: (): I64Builder => new I64Builder(),

  /**
   * Creates a new `U64` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new {@link U64Builder} instance
   */
  u64: (): U64Builder => new U64Builder(),

  /**
   * Creates a new `I128` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new {@link I128Builder} instance
   */
  i128: (): I128Builder => new I128Builder(),

  /**
   * Creates a new `U128` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new {@link U128Builder} instance
   */
  u128: (): U128Builder => new U128Builder(),

  /**
   * Creates a new `I256` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new {@link I256Builder} instance
   */
  i256: (): I256Builder => new I256Builder(),

  /**
   * Creates a new `U256` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new {@link U256Builder} instance
   */
  u256: (): U256Builder => new U256Builder(),

  /**
   * Creates a new `F32` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link F32Builder} instance
   */
  f32: (): F32Builder => new F32Builder(),

  /**
   * Creates a new `F64` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new {@link F64Builder} instance
   */
  f64: (): F64Builder => new F64Builder(),

  /**
   * Creates a new `Product` {@link AlgebraicType} to be used in table definitions. Product types in SpacetimeDB
   * are essentially the same as objects in JavaScript/TypeScript.
   * Properties of the object must also be {@link TypeBuilder}s.
   * Represented as an object with specific properties in TypeScript.
   *
   * @param name (optional) A display name for the product type. If omitted, an anonymous product type is created.
   * @param obj The object defining the properties of the type, whose property
   * values must be {@link TypeBuilder}s.
   * @returns A new {@link ProductBuilder} instance.
   */
  object: ((nameOrObj: any, maybeObj?: any) => {
    if (typeof nameOrObj === 'string') {
      if (!maybeObj) {
        throw new TypeError(
          'When providing a name, you must also provide the object.'
        );
      }
      return new ProductBuilder(maybeObj, nameOrObj);
    }
    return new ProductBuilder(nameOrObj, undefined);
  }) as {
    <Obj extends ElementsObj>(name: string, obj: Obj): ProductBuilder<Obj>;
    // TODO: Currently names are not optional
    // <Obj extends ElementsObj>(obj: Obj): ProductBuilder<Obj>;
  },

  /**
   * Creates a new `Row` {@link AlgebraicType} to be used in table definitions. Row types in SpacetimeDB
   * are similar to `Product` types, but are specifically used to define the schema of a table row.
   * Properties of the object must also be {@link TypeBuilder} or {@link ColumnBuilder}s.
   *
   * You can represent a `Row` as either a {@link RowObj} or an {@link RowBuilder} type when
   * defining a table schema.
   *
   * The {@link RowBuilder} type is useful when you want to create a type which can be used anywhere
   * a {@link TypeBuilder} is accepted, such as in nested objects or arrays, or as the argument
   * to a scheduled function.
   *
   * @param obj The object defining the properties of the row, whose property
   * values must be {@link TypeBuilder}s or {@link ColumnBuilder}s.
   * @returns A new {@link RowBuilder} instance
   */
  row: (<Obj extends RowObj>(
    nameOrObj: string | Obj,
    maybeObj?: Obj
  ): RowBuilder<Obj> => {
    const [obj, name] =
      typeof nameOrObj === 'string'
        ? [maybeObj!, nameOrObj]
        : [nameOrObj, undefined];
    return new RowBuilder(obj, name);
  }) as {
    <Obj extends RowObj>(obj: Obj): RowBuilder<Obj>;
    <Obj extends RowObj>(name: string, obj: Obj): RowBuilder<Obj>;
  },

  /**
   * Creates a new `Array` {@link AlgebraicType} to be used in table definitions.
   * Represented as an array in TypeScript.
   * @param element The element type of the array, which must be a `TypeBuilder`.
   * @returns A new {@link ArrayBuilder} instance
   */
  array<Element extends TypeBuilder<any, any>>(
    e: Element
  ): ArrayBuilder<Element> {
    return new ArrayBuilder(e);
  },

  enum: enumImpl,

  /**
   * This is a special helper function for conveniently creating {@link Product} type columns with no fields.
   *
   * @returns A new {@link ProductBuilder} instance with no fields.
   */
  unit(): UnitBuilder {
    return new UnitBuilder();
  },

  /**
   * Creates a lazily-evaluated {@link TypeBuilder}. This is useful for creating
   * recursive types, such as a tree or linked list.
   * @param thunk A function that returns a {@link TypeBuilder}.
   * @returns A proxy {@link TypeBuilder} that evaluates the thunk on first access.
   */
  lazy<Build extends () => TypeBuilder<any, any>>(
    thunk: Build
  ): ReturnType<Build> {
    type B = ReturnType<Build>;
    let cached: B | null = null;
    const get = (): B => (cached ??= thunk() as B);

    const proxy = new Proxy({} as unknown as B, {
      get(_t, prop, recv) {
        const target = get() as any;
        const val = Reflect.get(target, prop, recv);
        return typeof val === 'function' ? val.bind(target) : val;
      },
      set(_t, prop, value, recv) {
        return Reflect.set(get() as any, prop, value, recv);
      },
      has(_t, prop) {
        return prop in (get() as any);
      },
      ownKeys() {
        return Reflect.ownKeys(get() as any);
      },
      getOwnPropertyDescriptor(_t, prop) {
        return Object.getOwnPropertyDescriptor(get() as any, prop);
      },
      getPrototypeOf() {
        // makes `instanceof TypeBuilder` work if you care about it
        return Object.getPrototypeOf(get() as any);
      },
    }) as B;

    return proxy;
  },

  /**
   * This is a special helper function for conveniently creating {@link ScheduleAt} type columns.
   * @returns A new ColumnBuilder instance with the {@link ScheduleAt} type.
   */
  scheduleAt: (): ScheduleAtBuilder => {
    return new ScheduleAtBuilder();
  },

  /**
   * This is a convenience method for creating a column with the {@link Option} type.
   * You can create a column of the same type by constructing an enum with a `some` and `none` variant.
   * @param value The type of the value contained in the `some` variant of the `Option`.
   * @returns A new {@link OptionBuilder} instance with the {@link Option} type.
   */
  option<Value extends TypeBuilder<any, any>>(
    value: Value
  ): OptionBuilder<Value> {
    return new OptionBuilder(value);
  },

  /**
   * This is a convenience method for creating a column with the {@link Result} type.
   * You can create a column of the same type by constructing an enum with an `ok` and `err` variant.
   * @param ok The type of the value contained in the `ok` variant of the `Result`.
   * @param err The type of the value contained in the `err` variant of the `Result`.
   * @returns A new {@link ResultBuilder} instance with the {@link Result} type.
   */
  result<Ok extends TypeBuilder<any, any>, Err extends TypeBuilder<any, any>>(
    ok: Ok,
    err: Err
  ): ResultBuilder<Ok, Err> {
    return new ResultBuilder(ok, err);
  },

  /**
   * This is a convenience method for creating a column with the {@link Identity} type.
   * You can create a column of the same type by constructing an `object` with a single `__identity__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link Identity} type.
   */
  identity: (): IdentityBuilder => {
    return new IdentityBuilder();
  },

  /**
   * This is a convenience method for creating a column with the {@link ConnectionId} type.
   * You can create a column of the same type by constructing an `object` with a single `__connection_id__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link ConnectionId} type.
   */
  connectionId: (): ConnectionIdBuilder => {
    return new ConnectionIdBuilder();
  },

  /**
   * This is a convenience method for creating a column with the {@link Timestamp} type.
   * You can create a column of the same type by constructing an `object` with a single `__timestamp_micros_since_unix_epoch__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link Timestamp} type.
   */
  timestamp: (): TimestampBuilder => {
    return new TimestampBuilder();
  },

  /**
   * This is a convenience method for creating a column with the {@link TimeDuration} type.
   * You can create a column of the same type by constructing an `object` with a single `__time_duration_micros__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link TimeDuration} type.
   */
  timeDuration: (): TimeDurationBuilder => {
    return new TimeDurationBuilder();
  },

  /**
   * This is a convenience method for creating a column with the {@link Uuid} type.
   * You can create a column of the same type by constructing an `object` with a single `__uuid__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link Uuid} type.
   */
  uuid: (): UuidBuilder => {
    return new UuidBuilder();
  },

  /**
   * This is a convenience method for creating a column with the {@link ByteArray} type.
   * You can create a column of the same type by constructing an `array` of `u8`.
   * The TypeScript representation is {@link Uint8Array}.
   * @returns A new {@link ByteArrayBuilder} instance with the {@link ByteArray} type.
   */
  byteArray: (): ByteArrayBuilder => {
    return new ByteArrayBuilder();
  },
} as const;
export default t;
