import {
  AlgebraicType,
  ConnectionId,
  Identity,
  ScheduleAt,
  SumTypeVariant,
  TimeDuration,
  Timestamp,
  type AlgebraicTypeVariants,
} from '..';
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
export type InferTypeOfRow<
  T extends Record<
    string,
    ColumnBuilder<any, any, any> | TypeBuilder<any, any>
  >,
> = {
  [K in keyof T & string]: InferTypeOfTypeBuilder<CollapseColumn<T[K]>>;
};

type CollapseColumn<
  T extends TypeBuilder<any, any> | ColumnBuilder<any, any, any>,
> = T extends ColumnBuilder<any, any, any> ? T['typeBuilder'] : T;

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
type TypeScriptTypeFromElementsObj<Elements extends ElementsObj> = {
  [K in keyof Elements]: InferTypeOfTypeBuilder<Elements[K]>;
};

type VariantsObj = Record<string, TypeBuilder<any, any>>;

/**
 * A type which converts the elements of ElementsObj to a TypeScript object type.
 * It works by `Infer`ing the types of the column builders which are the values of
 * the keys in the object passed in.
 *
 * e.g. { A: I32TypeBuilder, B: StringBuilder } -> { tag: "A", value: number } | { tag: "B", value: string }
 */
type TypeScriptTypeFromVariantsObj<Variants extends VariantsObj> = {
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
export class TypeBuilder<Type, SpacetimeType extends AlgebraicType> {
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
  M extends ColumnMetadata = DefaultMetadata,
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
  M extends ColumnMetadata = DefaultMetadata,
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
  M extends ColumnMetadata = DefaultMetadata,
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
  M extends ColumnMetadata = DefaultMetadata,
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

export class U8Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U8>
  implements
    Indexable<number, AlgebraicTypeVariants.U8>,
    Uniqueable<number, AlgebraicTypeVariants.U8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U8>,
    AutoIncrementable<number, AlgebraicTypeVariants.U8>
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
}

export class U16Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U16>
  implements
    Indexable<number, AlgebraicTypeVariants.U16>,
    Uniqueable<number, AlgebraicTypeVariants.U16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U16>,
    AutoIncrementable<number, AlgebraicTypeVariants.U16>
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
}

export class U32Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.U32>
  implements
    Indexable<number, AlgebraicTypeVariants.U32>,
    Uniqueable<number, AlgebraicTypeVariants.U32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U32>,
    AutoIncrementable<number, AlgebraicTypeVariants.U32>
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
}

export class U64Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U64>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U64>,
    Uniqueable<bigint, AlgebraicTypeVariants.U64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U64>
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
}

export class U128Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U128>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U128>,
    Uniqueable<bigint, AlgebraicTypeVariants.U128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U128>
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
}

export class U256Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.U256>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U256>,
    Uniqueable<bigint, AlgebraicTypeVariants.U256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U256>
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
}

export class I8Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I8>
  implements
    Indexable<number, AlgebraicTypeVariants.I8>,
    Uniqueable<number, AlgebraicTypeVariants.I8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I8>,
    AutoIncrementable<number, AlgebraicTypeVariants.I8>
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
}

export class I16Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I16>
  implements
    Indexable<number, AlgebraicTypeVariants.I16>,
    Uniqueable<number, AlgebraicTypeVariants.I16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I16>,
    AutoIncrementable<number, AlgebraicTypeVariants.I16>
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
}

export class I32Builder
  extends TypeBuilder<number, AlgebraicTypeVariants.I32>
  implements
    TypeBuilder<number, AlgebraicTypeVariants.I32>,
    Indexable<number, AlgebraicTypeVariants.I32>,
    Uniqueable<number, AlgebraicTypeVariants.I32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I32>,
    AutoIncrementable<number, AlgebraicTypeVariants.I32>
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
}

export class I64Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I64>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I64>,
    Uniqueable<bigint, AlgebraicTypeVariants.I64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I64>
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
}

export class I128Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I128>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I128>,
    Uniqueable<bigint, AlgebraicTypeVariants.I128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I128>
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
}

export class I256Builder
  extends TypeBuilder<bigint, AlgebraicTypeVariants.I256>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I256>,
    Uniqueable<bigint, AlgebraicTypeVariants.I256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I256>
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
}

export class F32Builder extends TypeBuilder<number, AlgebraicTypeVariants.F32> {
  constructor() {
    super(AlgebraicType.F32);
  }
}

export class F64Builder extends TypeBuilder<number, AlgebraicTypeVariants.F64> {
  constructor() {
    super(AlgebraicType.F64);
  }
}

export class BoolBuilder
  extends TypeBuilder<boolean, AlgebraicTypeVariants.Bool>
  implements
    Indexable<boolean, AlgebraicTypeVariants.Bool>,
    Uniqueable<boolean, AlgebraicTypeVariants.Bool>,
    PrimaryKeyable<boolean, AlgebraicTypeVariants.Bool>
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
}

export class StringBuilder
  extends TypeBuilder<string, AlgebraicTypeVariants.String>
  implements
    Indexable<string, AlgebraicTypeVariants.String>,
    Uniqueable<string, AlgebraicTypeVariants.String>,
    PrimaryKeyable<string, AlgebraicTypeVariants.String>
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
}

export class ArrayBuilder<
  Element extends TypeBuilder<any, any>,
> extends TypeBuilder<
  Array<InferTypeOfTypeBuilder<Element>>,
  { tag: 'Array'; value: InferSpacetimeTypeOfTypeBuilder<Element> }
> {
  /**
   * The phantom element type of the array for TypeScript
   */
  readonly element!: Element;

  constructor(element: Element) {
    super(AlgebraicType.Array(element.algebraicType));
  }
}

export class ProductBuilder<Elements extends ElementsObj> extends TypeBuilder<
  TypeScriptTypeFromElementsObj<Elements>,
  {
    tag: 'Product';
    value: { elements: ElementsArrayFromElementsObj<Elements> };
  }
> {
  constructor(elements: Elements) {
    function elementsArrayFromElementsObj<Obj extends ElementsObj>(obj: Obj) {
      return Object.entries(obj).map(([name, { algebraicType }]) => ({
        name,
        algebraicType,
      }));
    }
    super(
      AlgebraicType.Product({
        elements: elementsArrayFromElementsObj(elements),
      })
    );
  }
}

export class SumBuilder<Variants extends VariantsObj> extends TypeBuilder<
  TypeScriptTypeFromVariantsObj<Variants>,
  { tag: 'Sum'; value: { variants: VariantsArrayFromVariantsObj<Variants> } }
> {
  constructor(variants: Variants) {
    function variantsArrayFromVariantsObj<Variants extends VariantsObj>(
      variants: Variants
    ) {
      return Object.entries(variants).map(([name, { algebraicType }]) => ({
        name,
        algebraicType,
      }));
    }
    super(
      AlgebraicType.Sum({
        variants: variantsArrayFromVariantsObj(variants),
      })
    );
  }
}

/**
 * The type of index types that can be applied to a column.
 * `undefined` is the default
 */
type IndexTypes = 'btree' | 'hash' | undefined;

/**
 * Metadata describing column constraints and index type
 */
export type ColumnMetadata = {
  isPrimaryKey?: true;
  isUnique?: true;
  isAutoIncrement?: true;
  isScheduleAt?: true;
  indexType?: IndexTypes;
};

/**
 * Default metadata state type for a newly created column
 */
type DefaultMetadata = object;

/**
 * Default metadata state value for a newly created column
 */
const defaultMetadata: ColumnMetadata = {};

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
  M extends ColumnMetadata = DefaultMetadata,
> {
  typeBuilder: TypeBuilder<Type, SpacetimeType>;
  columnMetadata: M;

  constructor(typeBuilder: TypeBuilder<Type, SpacetimeType>, metadata: M) {
    this.typeBuilder = typeBuilder;
    this.columnMetadata = metadata; //?? defaultMetadata;
  }
}

export class U8ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.U8, M>
  implements
    Indexable<number, AlgebraicTypeVariants.U8>,
    Uniqueable<number, AlgebraicTypeVariants.U8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U8>,
    AutoIncrementable<number, AlgebraicTypeVariants.U8>
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
}

export class U16ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.U16, M>
  implements
    Indexable<number, AlgebraicTypeVariants.U16>,
    Uniqueable<number, AlgebraicTypeVariants.U16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U16>,
    AutoIncrementable<number, AlgebraicTypeVariants.U16>
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
}

export class U32ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.U32, M>
  implements
    Indexable<number, AlgebraicTypeVariants.U32>,
    Uniqueable<number, AlgebraicTypeVariants.U32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.U32>,
    AutoIncrementable<number, AlgebraicTypeVariants.U32>
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
}

export class U64ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.U64, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U64>,
    Uniqueable<bigint, AlgebraicTypeVariants.U64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U64>
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
}

export class U128ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.U128, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U128>,
    Uniqueable<bigint, AlgebraicTypeVariants.U128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U128>
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
}

export class U256ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.U256, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.U256>,
    Uniqueable<bigint, AlgebraicTypeVariants.U256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.U256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.U256>
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
}

export class I8ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.I8, M>
  implements
    Indexable<number, AlgebraicTypeVariants.I8>,
    Uniqueable<number, AlgebraicTypeVariants.I8>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I8>,
    AutoIncrementable<number, AlgebraicTypeVariants.I8>
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
}

export class I16ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.I16, M>
  implements
    Indexable<number, AlgebraicTypeVariants.I16>,
    Uniqueable<number, AlgebraicTypeVariants.I16>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I16>,
    AutoIncrementable<number, AlgebraicTypeVariants.I16>
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
}

export class I32ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<number, AlgebraicTypeVariants.I32, M>
  implements
    Indexable<number, AlgebraicTypeVariants.I32>,
    Uniqueable<number, AlgebraicTypeVariants.I32>,
    PrimaryKeyable<number, AlgebraicTypeVariants.I32>,
    AutoIncrementable<number, AlgebraicTypeVariants.I32>
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
}

export class I64ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.I64, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I64>,
    Uniqueable<bigint, AlgebraicTypeVariants.I64>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I64>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I64>
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
}

export class I128ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.I128, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I128>,
    Uniqueable<bigint, AlgebraicTypeVariants.I128>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I128>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I128>
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
}

export class I256ColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<bigint, AlgebraicTypeVariants.I256, M>
  implements
    Indexable<bigint, AlgebraicTypeVariants.I256>,
    Uniqueable<bigint, AlgebraicTypeVariants.I256>,
    PrimaryKeyable<bigint, AlgebraicTypeVariants.I256>,
    AutoIncrementable<bigint, AlgebraicTypeVariants.I256>
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
}

export class F32ColumnBuilder<
  M extends ColumnMetadata = DefaultMetadata,
> extends ColumnBuilder<number, AlgebraicTypeVariants.F32, M> {}
export class F64ColumnBuilder<
  M extends ColumnMetadata = DefaultMetadata,
> extends ColumnBuilder<number, AlgebraicTypeVariants.F64, M> {}
export class BoolColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<boolean, AlgebraicTypeVariants.Bool, M>
  implements
    Indexable<boolean, AlgebraicTypeVariants.Bool>,
    Uniqueable<boolean, AlgebraicTypeVariants.Bool>,
    PrimaryKeyable<boolean, AlgebraicTypeVariants.Bool>
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
}

export class StringColumnBuilder<M extends ColumnMetadata = DefaultMetadata>
  extends ColumnBuilder<string, AlgebraicTypeVariants.String, M>
  implements
    Indexable<string, AlgebraicTypeVariants.String>,
    Uniqueable<string, AlgebraicTypeVariants.String>,
    PrimaryKeyable<string, AlgebraicTypeVariants.String>
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
}

export class ArrayColumnBuilder<
  Element extends TypeBuilder<any, any>,
  M extends ColumnMetadata = DefaultMetadata,
> extends ColumnBuilder<
  Array<InferTypeOfTypeBuilder<Element>>,
  { tag: 'Array'; value: InferSpacetimeTypeOfTypeBuilder<Element> },
  M
> {}

export class ProductColumnBuilder<
  Elements extends ElementsObj,
  M extends ColumnMetadata = DefaultMetadata,
> extends ColumnBuilder<
  TypeScriptTypeFromElementsObj<Elements>,
  {
    tag: 'Product';
    value: { elements: ElementsArrayFromElementsObj<Elements> };
  },
  M
> {}

export class SumColumnBuilder<
  Variants extends VariantsObj,
  M extends ColumnMetadata = DefaultMetadata,
> extends ColumnBuilder<
  TypeScriptTypeFromVariantsObj<Variants>,
  { tag: 'Sum'; value: { variants: VariantsArrayFromVariantsObj<Variants> } },
  M
> {}

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
const t = {
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
   * @param obj The object defining the properties of the type, whose property
   * values must be {@link TypeBuilder}s.
   * @returns A new {@link ProductBuilder} instance
   */
  object<Obj extends ElementsObj>(obj: Obj): ProductBuilder<Obj> {
    return new ProductBuilder(obj);
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
   * @param obj The object defining the variants of the enum, whose variant
   * types must be `TypeBuilder`s.
   * @returns A new {@link SumBuilder} instance
   */
  enum<Obj extends VariantsObj>(obj: Obj): SumBuilder<Obj> {
    return new SumBuilder(obj);
  },

  /**
   * This is a special helper function for conveniently creating {@link ScheduleAt} type columns.
   * @returns A new ColumnBuilder instance with the {@link ScheduleAt} type.
   */
  scheduleAt: (): ColumnBuilder<
    ScheduleAt,
    ReturnType<typeof ScheduleAt.getAlgebraicType>,
    Omit<ColumnMetadata, 'isScheduleAt'> & { isScheduleAt: true }
  > => {
    return new ColumnBuilder(
      new TypeBuilder(ScheduleAt.getAlgebraicType()),
      set(defaultMetadata, { isScheduleAt: true })
    );
  },

  /**
   * This is a convenience method for creating a column with the {@link Identity} type.
   * You can create a column of the same type by constructing an `object` with a single `__identity__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link Identity} type.
   */
  identity: (): TypeBuilder<
    Identity,
    AlgebraicTypeVariants.Product & { value: typeof Identity }
  > => {
    return new TypeBuilder(Identity.getAlgebraicType() as any);
  },

  /**
   * This is a convenience method for creating a column with the {@link ConnectionId} type.
   * You can create a column of the same type by constructing an `object` with a single `__connection_id__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link ConnectionId} type.
   */
  connectionId: (): TypeBuilder<
    string,
    AlgebraicTypeVariants.Product & { value: typeof ConnectionId }
  > => {
    return new TypeBuilder(ConnectionId.getAlgebraicType() as any);
  },

  /**
   * This is a convenience method for creating a column with the {@link Timestamp} type.
   * You can create a column of the same type by constructing an `object` with a single `__timestamp_micros_since_unix_epoch__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link Timestamp} type.
   */
  timestamp: (): TypeBuilder<
    Timestamp,
    AlgebraicTypeVariants.Product & { value: typeof Timestamp }
  > => {
    return new TypeBuilder(Timestamp.getAlgebraicType() as any);
  },

  /**
   * This is a convenience method for creating a column with the {@link TimeDuration} type.
   * You can create a column of the same type by constructing an `object` with a single `__time_duration_micros__` element.
   * @returns A new {@link TypeBuilder} instance with the {@link TimeDuration} type.
   */
  timeDuration: (): TypeBuilder<
    TimeDuration,
    AlgebraicTypeVariants.Product & { value: typeof TimeDuration }
  > => {
    return new TypeBuilder(TimeDuration.getAlgebraicType() as any);
  },
} as const;
export default t;
