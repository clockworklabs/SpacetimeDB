import {
  AlgebraicType,
  ConnectionId,
  Identity,
  ScheduleAt,
  TimeDuration,
  Timestamp,
  Option,
  type AlgebraicTypeVariants,
  type ConnectionIdAlgebraicType,
  type IdentityAlgebraicType,
  type TimeDurationAlgebraicType,
  type TimestampAlgebraicType,
} from '..';
import type { OptionAlgebraicType } from '../lib/option';
import { addType, MODULE_DEF } from './schema';
import type { CoerceRow } from './table';
import { set, type Set } from './type_util';

/**
 * Helper type to extract the TypeScript type from a TypeBuilder
 */
export type InferTypeOfTypeBuilder<T extends TypeBuilder<any, any>> =
  T extends TypeBuilder<infer U, any> ? U : never;

/**
 * Helper type to extract the Spacetime type from a TypeBuilder
 */
export type InferSpacetimeTypeOfTypeBuilder<T extends TypeBuilder<any, any>> =
  T extends TypeBuilder<any, infer U> ? U : never;

/**
 * Helper type to extract the TypeScript type from a TypeBuilder
 */
export type Infer<T extends TypeBuilder<any, any>> = InferTypeOfTypeBuilder<T>;

/**
 * Helper type to extract the type of a row from an object.
 */
export type InferTypeOfRow<T extends RowObj> = {
  [K in keyof T & string]: InferTypeOfTypeBuilder<CollapseColumn<T[K]>>;
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
  TypeBuilder<any, any> | ColumnBuilder<any, any, any>
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
type ElementsObj = Record<string, TypeBuilder<any, any>>;

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

type VariantsObj = Record<string, TypeBuilder<any, any>>;
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
type UnitBuilder = ProductBuilder<{}>;
type SimpleVariantsObj = Record<string, UnitBuilder>;

/**
 * A type which converts the elements of ElementsObj to a TypeScript object type.
 * It works by `Infer`ing the types of the column builders which are the values of
 * the keys in the object passed in.
 *
 * e.g. { A: I32TypeBuilder, B: StringBuilder } -> { tag: "A", value: number } | { tag: "B", value: string }
 */
type EnumType<Variants extends VariantsObj> = {
  [K in keyof Variants]: { tag: K; value: InferTypeOfTypeBuilder<Variants[K]> };
}[keyof Variants];

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
  readonly algebraicType: SpacetimeType | AlgebraicTypeVariants.Ref;

  constructor(algebraicType: SpacetimeType | AlgebraicTypeVariants.Ref) {
    this.algebraicType = algebraicType;
  }

  resolveType(): SpacetimeType {
    let ty: AlgebraicType = this.algebraicType;
    while (ty.tag === 'Ref') ty = MODULE_DEF.typespace.types[ty.value];
    return ty as SpacetimeType;
  }

  optional(): OptionBuilder<typeof this> {
    return new OptionBuilder(this);
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
 * - Marking a column as a primary key is mutually exclusive with certain other metadata flags,
 *   such as `isAutoIncrement` or `isUnique`, depending on the database schema rules.
 */
interface PrimaryKeyable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify this column as primary key
   */
  primaryKey(): ColumnBuilder<
    Type,
    SpacetimeType,
    Set<M, 'isPrimaryKey', true>
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
 * - Marking a column as unique is mutually exclusive with certain other metadata flags,
 *   such as `isAutoIncrement` or `isPrimaryKey`, depending on the database schema rules.
 */
interface Uniqueable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify this column as unique
   */
  unique(): ColumnBuilder<Type, SpacetimeType, Set<M, 'isUnique', true>>;
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
  index(): ColumnBuilder<Type, SpacetimeType, Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): ColumnBuilder<Type, SpacetimeType, Set<M, 'indexType', N>>;
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
 * - Marking a column as auto-incrementing is mutually exclusive with certain other metadata flags,
 *   such as `isUnique` or `isPrimaryKey`, depending on the database schema rules.
 */
interface AutoIncrementable<
  Type,
  SpacetimeType extends AlgebraicType,
  M extends ColumnMetadata<Type> = DefaultMetadata,
> {
  /**
   * Specify this column as auto-incrementing
   */
  autoInc(): ColumnBuilder<
    Type,
    SpacetimeType,
    Set<M, 'isAutoIncrement', true>
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
   */
  default(
    value: Type
  ): ColumnBuilder<Type, SpacetimeType, Set<M, 'defaultValue', Type>>;
}

export class U8Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U8>
  implements
    Indexable<number, AlgebraicTypeVariants.U8>,
    Uniqueable<number, AlgebraicTypeVariants.U8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U8>,
    AutoIncrementable<number, AlgebraicTypeVariants.U8>,
    Defaultable<number, AlgebraicTypeVariants.U8>
{
  constructor() {
    super(AlgebraicType.U8);
  }
  index(): U8ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U8ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U8ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U8ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new U8ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U8ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U8ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U8ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new U8ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class U16Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U16>
  implements
    Indexable<number, AlgebraicTypeVariants.U16>,
    Uniqueable<number, AlgebraicTypeVariants.U16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U16>,
    AutoIncrementable<number, AlgebraicTypeVariants.U16>,
    Defaultable<number, AlgebraicTypeVariants.U16>
{
  constructor() {
    super(AlgebraicType.U16);
  }
  index(): U16ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U16ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U16ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U16ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new U16ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U16ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U16ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U16ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new U16ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class U32Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U32>
  implements
    Indexable<number, AlgebraicTypeVariants.U32>,
    Uniqueable<number, AlgebraicTypeVariants.U32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U32>,
    AutoIncrementable<number, AlgebraicTypeVariants.U32>,
    Defaultable<number, AlgebraicTypeVariants.U32>
{
  constructor() {
    super(AlgebraicType.U32);
  }
  index(): U32ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U32ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U32ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U32ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new U32ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U32ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U32ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): U32ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new U32ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class U64Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U64>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U64>,
    Uniqueable<bigint, AlgebraicTypeVariants.U64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U64>,
    Defaultable<bigint, AlgebraicTypeVariants.U64>
{
  constructor() {
    super(AlgebraicType.U64);
  }
  index(): U64ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U64ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U64ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U64ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new U64ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): U64ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U64ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U64ColumnBuilder<Set<DefaultMetadata, 'defaultValue', bigint>> {
    return new U64ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class U128Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U128>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U128>,
    Uniqueable<bigint, AlgebraicTypeVariants.U128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U128>,
    Defaultable<bigint, AlgebraicTypeVariants.U128>
{
  constructor() {
    super(AlgebraicType.U128);
  }
  index(): U128ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U128ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U128ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U128ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): U128ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U128ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U128ColumnBuilder<Set<DefaultMetadata, 'defaultValue', bigint>> {
    return new U128ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class U256Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U256>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U256>,
    Uniqueable<bigint, AlgebraicTypeVariants.U256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U256>,
    Defaultable<bigint, AlgebraicTypeVariants.U256>
{
  constructor() {
    super(AlgebraicType.U256);
  }
  index(): U256ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U256ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U256ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): U256ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): U256ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U256ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): U256ColumnBuilder<Set<DefaultMetadata, 'defaultValue', bigint>> {
    return new U256ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class I8Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I8>
  implements
    Indexable<number, AlgebraicTypeVariants.I8>,
    Uniqueable<number, AlgebraicTypeVariants.I8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I8>,
    AutoIncrementable<number, AlgebraicTypeVariants.I8>,
    Defaultable<number, AlgebraicTypeVariants.I8>
{
  constructor() {
    super(AlgebraicType.I8);
  }
  index(): I8ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I8ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I8ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I8ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new I8ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I8ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I8ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I8ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new I8ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class I16Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I16>
  implements
    Indexable<number, AlgebraicTypeVariants.I16>,
    Uniqueable<number, AlgebraicTypeVariants.I16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I16>,
    AutoIncrementable<number, AlgebraicTypeVariants.I16>,
    Defaultable<number, AlgebraicTypeVariants.I16>
{
  constructor() {
    super(AlgebraicType.I16);
  }
  index(): I16ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I16ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I16ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I16ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new I16ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I16ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I16ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I16ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new I16ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
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
    Defaultable<number, AlgebraicTypeVariants.I32>
{
  constructor() {
    super(AlgebraicType.I32);
  }
  index(): I32ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I32ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I32ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I32ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new I32ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I32ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I32ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: number
  ): I32ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new I32ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class I64Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I64>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I64>,
    Uniqueable<bigint, AlgebraicTypeVariants.I64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I64>,
    Defaultable<bigint, AlgebraicTypeVariants.I64>
{
  constructor() {
    super(AlgebraicType.I64);
  }
  index(): I64ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I64ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I64ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I64ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new I64ColumnBuilder(this, set(defaultMetadata, { isUnique: true }));
  }
  primaryKey(): I64ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I64ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I64ColumnBuilder<Set<DefaultMetadata, 'defaultValue', bigint>> {
    return new I64ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class I128Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I128>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I128>,
    Uniqueable<bigint, AlgebraicTypeVariants.I128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I128>,
    Defaultable<bigint, AlgebraicTypeVariants.I128>
{
  constructor() {
    super(AlgebraicType.I128);
  }
  index(): I128ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I128ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I128ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I128ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): I128ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I128ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I128ColumnBuilder<Set<DefaultMetadata, 'defaultValue', bigint>> {
    return new I128ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class I256Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I256>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I256>,
    Uniqueable<bigint, AlgebraicTypeVariants.I256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I256>,
    Defaultable<bigint, AlgebraicTypeVariants.I256>
{
  constructor() {
    super(AlgebraicType.I256);
  }
  index(): I256ColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I256ColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I256ColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): I256ColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): I256ColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I256ColumnBuilder<Set<DefaultMetadata, 'isAutoIncrement', true>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: bigint
  ): I256ColumnBuilder<Set<DefaultMetadata, 'defaultValue', bigint>> {
    return new I256ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class F32Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.F32>
  implements Defaultable<number, AlgebraicTypeVariants.F32>
{
  constructor() {
    super(AlgebraicType.F32);
  }
  default(
    value: number
  ): F32ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new F32ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class F64Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.F64>
  implements Defaultable<number, AlgebraicTypeVariants.F64>
{
  constructor() {
    super(AlgebraicType.F64);
  }
  default(
    value: number
  ): F64ColumnBuilder<Set<DefaultMetadata, 'defaultValue', number>> {
    return new F64ColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class BoolBuilder
  extends TypeBuilder<boolean, AlgebraicTypeVariants.Bool>
  implements
    Indexable<boolean, AlgebraicTypeVariants.Bool>,
    Uniqueable<boolean, AlgebraicTypeVariants.Bool>,
    PrimaryKeyable<boolean, AlgebraicTypeVariants.Bool>,
    Defaultable<boolean, AlgebraicTypeVariants.Bool>
{
  constructor() {
    super(AlgebraicType.Bool);
  }
  index(): BoolColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): BoolColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): BoolColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): BoolColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): BoolColumnBuilder<Set<DefaultMetadata, 'isPrimaryKey', true>> {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: boolean
  ): BoolColumnBuilder<Set<DefaultMetadata, 'defaultValue', boolean>> {
    return new BoolColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class StringBuilder
  extends TypeBuilder<string, AlgebraicTypeVariants.String>
  implements
    Indexable<string, AlgebraicTypeVariants.String>,
    Uniqueable<string, AlgebraicTypeVariants.String>,
    PrimaryKeyable<string, AlgebraicTypeVariants.String>,
    Defaultable<string, AlgebraicTypeVariants.String>
{
  constructor() {
    super(AlgebraicType.String);
  }
  index(): StringColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): StringColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): StringColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): StringColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): StringColumnBuilder<
    Set<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: string
  ): StringColumnBuilder<Set<DefaultMetadata, 'defaultValue', string>> {
    return new StringColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class ArrayBuilder<Element extends TypeBuilder<any, any>>
  extends TypeBuilder<
    Array<InferTypeOfTypeBuilder<Element>>,
    { tag: 'Array'; value: InferSpacetimeTypeOfTypeBuilder<Element> }
  >
  implements Defaultable<Array<InferTypeOfTypeBuilder<Element>>, any>
{
  /**
   * The phantom element type of the array for TypeScript
   */
  readonly element!: Element;

  constructor(element: Element) {
    super(AlgebraicType.Array(element.algebraicType));
  }
  default(
    value: Array<InferTypeOfTypeBuilder<Element>>
  ): ArrayColumnBuilder<Element, Set<DefaultMetadata, 'defaultValue', any>> {
    return new ArrayColumnBuilder(
      this.element,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class OptionBuilder<Value extends TypeBuilder<any, any>>
  extends TypeBuilder<
    InferTypeOfTypeBuilder<Value> | undefined,
    OptionAlgebraicType
  >
  implements
    Defaultable<InferTypeOfTypeBuilder<Value> | undefined, OptionAlgebraicType>
{
  /**
   * The phantom value type of the option for TypeScript
   */
  readonly value!: Value;

  constructor(value: Value) {
    let innerType: AlgebraicType;
    if (value instanceof ColumnBuilder) {
      innerType = value.typeBuilder.algebraicType;
    } else {
      innerType = value.algebraicType;
    }
    super(Option.getAlgebraicType(innerType));
  }
  default(
    value: InferTypeOfTypeBuilder<Value> | undefined
  ): OptionColumnBuilder<
    InferTypeOfTypeBuilder<Value>,
    Set<
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
}

export class ProductBuilder<Elements extends ElementsObj>
  extends TypeBuilder<
    ObjectType<Elements>,
    {
      tag: 'Product';
      value: { elements: ElementsArrayFromElementsObj<Elements> };
    }
  >
  implements Defaultable<ObjectType<Elements>, any>
{
  constructor(elements: Elements, name?: string) {
    function elementsArrayFromElementsObj<Obj extends ElementsObj>(obj: Obj) {
      return Object.entries(obj).map(([name, { algebraicType }]) => ({
        name,
        algebraicType,
      }));
    }
    super(
      addType(
        name,
        AlgebraicType.Product({
          elements: elementsArrayFromElementsObj(elements),
        })
      )
    );
  }
  default(
    value: ObjectType<Elements>
  ): ProductColumnBuilder<Elements, Set<DefaultMetadata, 'defaultValue', any>> {
    return new ProductColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class RowBuilder<Row extends RowObj> extends TypeBuilder<
  RowType<Row>,
  {
    tag: 'Product';
    value: { elements: ElementsArrayFromRowObj<Row> };
  }
> {
  row: CoerceRow<Row>;
  nameProvided: boolean;
  constructor(row: Row, name?: string) {
    const mappedRow = Object.fromEntries(
      Object.entries(row).map(([name, builder]) => [
        name,
        builder instanceof ColumnBuilder
          ? builder
          : new ColumnBuilder(builder, {}),
      ])
    ) as CoerceRow<Row>;

    const elements = Object.entries(mappedRow).map(([name, builder]) => ({
      name,
      algebraicType: builder.typeBuilder.algebraicType,
    })) as ElementsArrayFromRowObj<Row>;

    super(addType(name, AlgebraicType.Product({ elements })));
    this.nameProvided = name != null;

    this.row = mappedRow;
  }
}

export class SumBuilder<Variants extends VariantsObj> extends TypeBuilder<
  EnumType<Variants>,
  { tag: 'Sum'; value: { variants: VariantsArrayFromVariantsObj<Variants> } }
> {
  constructor(variants: Variants, name?: string) {
    function variantsArrayFromVariantsObj<Variants extends VariantsObj>(
      variants: Variants
    ) {
      return Object.entries(variants).map(([name, { algebraicType }]) => ({
        name,
        algebraicType,
      }));
    }
    super(
      addType(
        name,
        AlgebraicType.Sum({
          variants: variantsArrayFromVariantsObj(variants),
        })
      )
    );
  }
  default(
    value: EnumType<Variants>
  ): SumColumnBuilder<Variants, Set<DefaultMetadata, 'defaultValue', any>> {
    return new SumColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class SimpleSumBuilder<Variants extends SimpleVariantsObj>
  extends SumBuilder<Variants>
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
    Set<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): SimpleSumColumnBuilder<Variants, Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): SimpleSumColumnBuilder<
    Variants,
    Set<DefaultMetadata, 'indexType', IndexTypes>
  > {
    return new SimpleSumColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  primaryKey(): SimpleSumColumnBuilder<
    Variants,
    Set<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new SimpleSumColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
}

export class IdentityBuilder
  extends TypeBuilder<Identity, IdentityAlgebraicType>
  implements
    Indexable<Identity, IdentityAlgebraicType>,
    Uniqueable<Identity, IdentityAlgebraicType>,
    PrimaryKeyable<Identity, IdentityAlgebraicType>,
    Defaultable<Identity, IdentityAlgebraicType>
{
  constructor() {
    super(Identity.getAlgebraicType());
  }
  index(): IdentityColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): IdentityColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): IdentityColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): IdentityColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): IdentityColumnBuilder<
    Set<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): IdentityColumnBuilder<
    Set<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: Identity
  ): IdentityColumnBuilder<Set<DefaultMetadata, 'defaultValue', Identity>> {
    return new IdentityColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class ConnectionIdBuilder
  extends TypeBuilder<ConnectionId, ConnectionIdAlgebraicType>
  implements
    Indexable<ConnectionId, ConnectionIdAlgebraicType>,
    Uniqueable<ConnectionId, ConnectionIdAlgebraicType>,
    PrimaryKeyable<ConnectionId, ConnectionIdAlgebraicType>,
    Defaultable<ConnectionId, ConnectionIdAlgebraicType>
{
  constructor() {
    super(ConnectionId.getAlgebraicType());
  }
  index(): ConnectionIdColumnBuilder<
    Set<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): ConnectionIdColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): ConnectionIdColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): ConnectionIdColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): ConnectionIdColumnBuilder<
    Set<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): ConnectionIdColumnBuilder<
    Set<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: ConnectionId
  ): ConnectionIdColumnBuilder<
    Set<DefaultMetadata, 'defaultValue', ConnectionId>
  > {
    return new ConnectionIdColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class TimestampBuilder
  extends TypeBuilder<Timestamp, TimestampAlgebraicType>
  implements
    Indexable<Timestamp, TimestampAlgebraicType>,
    Uniqueable<Timestamp, TimestampAlgebraicType>,
    PrimaryKeyable<Timestamp, TimestampAlgebraicType>,
    Defaultable<Timestamp, TimestampAlgebraicType>
{
  constructor() {
    super(Timestamp.getAlgebraicType());
  }
  index(): TimestampColumnBuilder<Set<DefaultMetadata, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimestampColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimestampColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): TimestampColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimestampColumnBuilder<
    Set<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): TimestampColumnBuilder<
    Set<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: Timestamp
  ): TimestampColumnBuilder<Set<DefaultMetadata, 'defaultValue', Timestamp>> {
    return new TimestampColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
  }
}

export class TimeDurationBuilder
  extends TypeBuilder<TimeDuration, TimeDurationAlgebraicType>
  implements
    Indexable<TimeDuration, TimeDurationAlgebraicType>,
    Uniqueable<TimeDuration, TimeDurationAlgebraicType>,
    PrimaryKeyable<TimeDuration, TimeDurationAlgebraicType>,
    Defaultable<TimeDuration, TimeDurationAlgebraicType>
{
  constructor() {
    super(TimeDuration.getAlgebraicType());
  }
  index(): TimeDurationColumnBuilder<
    Set<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimeDurationColumnBuilder<Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimeDurationColumnBuilder<Set<DefaultMetadata, 'indexType', IndexTypes>> {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { indexType: algorithm })
    );
  }
  unique(): TimeDurationColumnBuilder<Set<DefaultMetadata, 'isUnique', true>> {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimeDurationColumnBuilder<
    Set<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): TimeDurationColumnBuilder<
    Set<DefaultMetadata, 'isAutoIncrement', true>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { isAutoIncrement: true })
    );
  }
  default(
    value: TimeDuration
  ): TimeDurationColumnBuilder<
    Set<DefaultMetadata, 'defaultValue', TimeDuration>
  > {
    return new TimeDurationColumnBuilder(
      this,
      set(defaultMetadata, { defaultValue: value })
    );
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
  isScheduleAt?: true;
  indexType?: IndexTypes;
  defaultValue?: Type;
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
}

export class U8ColumnBuilder<M extends ColumnMetadata<number> = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.U8, M>
  implements
    Indexable<number, AlgebraicTypeVariants.U8>,
    Uniqueable<number, AlgebraicTypeVariants.U8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U8>,
    AutoIncrementable<number, AlgebraicTypeVariants.U8>,
    Defaultable<number, AlgebraicTypeVariants.U8>
{
  index(): U8ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U8ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U8ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U8ColumnBuilder<Set<M, 'isUnique', true>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U8ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U8ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new U8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: number): U8ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new U8ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<number, AlgebraicTypeVariants.U16>
{
  index(): U16ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U16ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U16ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U16ColumnBuilder<Set<M, 'isUnique', true>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U16ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U16ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new U16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: number): U16ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new U16ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<number, AlgebraicTypeVariants.U32>
{
  index(): U32ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U32ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U32ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U32ColumnBuilder<Set<M, 'isUnique', true>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U32ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U32ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new U32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: number): U32ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new U32ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<bigint, AlgebraicTypeVariants.U64>
{
  index(): U64ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U64ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U64ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U64ColumnBuilder<Set<M, 'isUnique', true>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U64ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U64ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new U64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: bigint): U64ColumnBuilder<Set<M, 'defaultValue', bigint>> {
    return new U64ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<bigint, AlgebraicTypeVariants.U128>
{
  index(): U128ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U128ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U128ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U128ColumnBuilder<Set<M, 'isUnique', true>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U128ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U128ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new U128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: bigint): U128ColumnBuilder<Set<M, 'defaultValue', bigint>> {
    return new U128ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<bigint, AlgebraicTypeVariants.U256>
{
  index(): U256ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): U256ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): U256ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): U256ColumnBuilder<Set<M, 'isUnique', true>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): U256ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): U256ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new U256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: bigint): U256ColumnBuilder<Set<M, 'defaultValue', bigint>> {
    return new U256ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
  }
}

export class I8ColumnBuilder<M extends ColumnMetadata<number> = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.I8, M>
  implements
    Indexable<number, AlgebraicTypeVariants.I8>,
    Uniqueable<number, AlgebraicTypeVariants.I8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I8>,
    AutoIncrementable<number, AlgebraicTypeVariants.I8>,
    Defaultable<number, AlgebraicTypeVariants.I8>
{
  index(): I8ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I8ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I8ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I8ColumnBuilder<Set<M, 'isUnique', true>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I8ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I8ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new I8ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: number): I8ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new I8ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<number, AlgebraicTypeVariants.I16>
{
  index(): I16ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I16ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I16ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I16ColumnBuilder<Set<M, 'isUnique', true>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I16ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I16ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new I16ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: number): I16ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new I16ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<number, AlgebraicTypeVariants.I32>
{
  index(): I32ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I32ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I32ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I32ColumnBuilder<Set<M, 'isUnique', true>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I32ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I32ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new I32ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: number): I32ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new I32ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<bigint, AlgebraicTypeVariants.I64>
{
  index(): I64ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I64ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I64ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I64ColumnBuilder<Set<M, 'isUnique', true>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I64ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I64ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new I64ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: bigint): I64ColumnBuilder<Set<M, 'defaultValue', bigint>> {
    return new I64ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<bigint, AlgebraicTypeVariants.I128>
{
  index(): I128ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I128ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I128ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I128ColumnBuilder<Set<M, 'isUnique', true>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I128ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I128ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new I128ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: bigint): I128ColumnBuilder<Set<M, 'defaultValue', bigint>> {
    return new I128ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<bigint, AlgebraicTypeVariants.I256>
{
  index(): I256ColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): I256ColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): I256ColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): I256ColumnBuilder<Set<M, 'isUnique', true>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): I256ColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  autoInc(): I256ColumnBuilder<Set<M, 'isAutoIncrement', true>> {
    return new I256ColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isAutoIncrement: true })
    );
  }
  default(value: bigint): I256ColumnBuilder<Set<M, 'defaultValue', bigint>> {
    return new I256ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
  }
}

export class F32ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.F32, M>
  implements Defaultable<number, AlgebraicTypeVariants.F32>
{
  default(value: number): F32ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new F32ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
  }
}

export class F64ColumnBuilder<
    M extends ColumnMetadata<number> = DefaultMetadata,
  >
  extends ColumnBuilder<number, AlgebraicTypeVariants.F64, M>
  implements Defaultable<number, AlgebraicTypeVariants.F64>
{
  default(value: number): F64ColumnBuilder<Set<M, 'defaultValue', number>> {
    return new F64ColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<boolean, AlgebraicTypeVariants.Bool>
{
  index(): BoolColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): BoolColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): BoolColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): BoolColumnBuilder<Set<M, 'isUnique', true>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): BoolColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new BoolColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(value: boolean): BoolColumnBuilder<Set<M, 'defaultValue', boolean>> {
    return new BoolColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<string, AlgebraicTypeVariants.String>
{
  index(): StringColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): StringColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): StringColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): StringColumnBuilder<Set<M, 'isUnique', true>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): StringColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new StringColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(value: string): StringColumnBuilder<Set<M, 'defaultValue', string>> {
    return new StringColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    >
{
  default(
    value: Array<InferTypeOfTypeBuilder<Element>>
  ): ArrayColumnBuilder<
    Element,
    Set<M, 'defaultValue', Array<InferTypeOfTypeBuilder<Element>>>
  > {
    return new ArrayColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    OptionAlgebraicType,
    M
  >
  implements
    Defaultable<InferTypeOfTypeBuilder<Value> | undefined, OptionAlgebraicType>
{
  default(
    value: InferTypeOfTypeBuilder<Value> | undefined
  ): OptionColumnBuilder<
    InferTypeOfTypeBuilder<Value>,
    Set<M, 'defaultValue', InferTypeOfTypeBuilder<Value> | undefined>
  > {
    return new OptionColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
  }
}

export class ProductColumnBuilder<
    Elements extends ElementsObj,
    M extends ColumnMetadata<ObjectType<Elements>> = DefaultMetadata,
  >
  extends ColumnBuilder<
    ObjectType<Elements>,
    {
      tag: 'Product';
      value: { elements: ElementsArrayFromElementsObj<Elements> };
    },
    M
  >
  implements Defaultable<ObjectType<Elements>, AlgebraicTypeVariants.Product>
{
  default(
    value: ObjectType<Elements>
  ): ProductColumnBuilder<Elements, Set<DefaultMetadata, 'defaultValue', any>> {
    return new ProductColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
    );
  }
}

export class SumColumnBuilder<
    Variants extends VariantsObj,
    M extends ColumnMetadata<EnumType<Variants>> = DefaultMetadata,
  >
  extends ColumnBuilder<
    EnumType<Variants>,
    { tag: 'Sum'; value: { variants: VariantsArrayFromVariantsObj<Variants> } },
    M
  >
  implements Defaultable<EnumType<Variants>, AlgebraicTypeVariants.Sum>
{
  default(
    value: EnumType<Variants>
  ): SumColumnBuilder<Variants, Set<DefaultMetadata, 'defaultValue', any>> {
    return new SumColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { defaultValue: value })
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
    Set<DefaultMetadata, 'indexType', 'btree'>
  >;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): SimpleSumColumnBuilder<Variants, Set<DefaultMetadata, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): SimpleSumColumnBuilder<
    Variants,
    Set<DefaultMetadata, 'indexType', IndexTypes>
  > {
    return new SimpleSumColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  primaryKey(): SimpleSumColumnBuilder<
    Variants,
    Set<DefaultMetadata, 'isPrimaryKey', true>
  > {
    return new SimpleSumColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
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
    Defaultable<Identity, IdentityAlgebraicType>
{
  index(): IdentityColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): IdentityColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): IdentityColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): IdentityColumnBuilder<Set<M, 'isUnique', true>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): IdentityColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new IdentityColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: Identity
  ): IdentityColumnBuilder<Set<M, 'defaultValue', Identity>> {
    return new IdentityColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<ConnectionId, ConnectionIdAlgebraicType>
{
  index(): ConnectionIdColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): ConnectionIdColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): ConnectionIdColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): ConnectionIdColumnBuilder<Set<M, 'isUnique', true>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): ConnectionIdColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new ConnectionIdColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: ConnectionId
  ): ConnectionIdColumnBuilder<Set<M, 'defaultValue', ConnectionId>> {
    return new ConnectionIdColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<Timestamp, TimestampAlgebraicType>
{
  index(): TimestampColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimestampColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimestampColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): TimestampColumnBuilder<Set<M, 'isUnique', true>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimestampColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new TimestampColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: Timestamp
  ): TimestampColumnBuilder<Set<M, 'defaultValue', Timestamp>> {
    return new TimestampColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
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
    Defaultable<TimeDuration, TimeDurationAlgebraicType>
{
  index(): TimeDurationColumnBuilder<Set<M, 'indexType', 'btree'>>;
  index<N extends NonNullable<IndexTypes>>(
    algorithm: N
  ): TimeDurationColumnBuilder<Set<M, 'indexType', N>>;
  index(
    algorithm: IndexTypes = 'btree'
  ): TimeDurationColumnBuilder<Set<M, 'indexType', IndexTypes>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { indexType: algorithm })
    );
  }
  unique(): TimeDurationColumnBuilder<Set<M, 'isUnique', true>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isUnique: true })
    );
  }
  primaryKey(): TimeDurationColumnBuilder<Set<M, 'isPrimaryKey', true>> {
    return new TimeDurationColumnBuilder(
      this.typeBuilder,
      set(this.columnMetadata, { isPrimaryKey: true })
    );
  }
  default(
    value: TimeDuration
  ): TimeDurationColumnBuilder<Set<M, 'defaultValue', TimeDuration>> {
    return new TimeDurationColumnBuilder(this.typeBuilder, {
      ...this.columnMetadata,
      defaultValue: value,
    });
  }
}

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

  /**
   * Creates a new `Sum` {@link AlgebraicType} to be used in table definitions. Sum types in SpacetimeDB
   * are similar to enums or unions in languages like Rust or TypeScript respectively.
   * Each variant of the enum must be a {@link TypeBuilder}.
   * Represented as a union of string literals in TypeScript.
   *
   * @param name (optional) A display name for the sum type. If omitted, an anonymous sum type is created.
   * @param obj The object defining the variants of the enum, whose variant
   * types must be {@link TypeBuilder}s.
   * @returns A new {@link SumBuilder} instance.
   */
  enum: ((nameOrObj: any, maybeObj?: any) => {
    let obj: VariantsObj = nameOrObj;
    let name: string | undefined = undefined;
    if (typeof nameOrObj === 'string') {
      if (!maybeObj) {
        throw new TypeError(
          'When providing a name, you must also provide the variants object.'
        );
      }
      obj = maybeObj;
      name = nameOrObj;
    }
    if (
      Object.values(obj).every(x => {
        const ty: AlgebraicType = x.resolveType();
        return ty.tag === 'Product' && ty.value.elements.length === 0;
      })
    ) {
      return new SimpleSumBuilder(obj as SimpleVariantsObj, name);
    }
    return new SumBuilder(obj, name);
  }) as {
    <Obj extends SimpleVariantsObj>(
      name: string,
      obj: Obj
    ): SimpleSumBuilder<Obj>;
    <Obj extends VariantsObj>(name: string, obj: Obj): SumBuilder<Obj>;
    // TODO: Currently names are not optional
    // <Obj extends VariantsObj>(obj: Obj): SumBuilder<Obj>;
  },

  /**
   * This is a special helper function for conveniently creating {@link Product} type columns with no fields.
   *
   * @returns A new {@link ProductBuilder} instance with no fields.
   */
  unit(): UnitBuilder {
    return new ProductBuilder({});
  },

  /**
   * This is a special helper function for conveniently creating {@link ScheduleAt} type columns.
   * @returns A new ColumnBuilder instance with the {@link ScheduleAt} type.
   */
  scheduleAt: (): ColumnBuilder<
    ScheduleAt,
    ReturnType<typeof ScheduleAt.getAlgebraicType>,
    Omit<ColumnMetadata<ScheduleAt>, 'isScheduleAt'> & { isScheduleAt: true }
  > => {
    return new ColumnBuilder(
      new TypeBuilder(ScheduleAt.getAlgebraicType()),
      set(defaultMetadata, { isScheduleAt: true })
    );
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
} as const;
export default t;
