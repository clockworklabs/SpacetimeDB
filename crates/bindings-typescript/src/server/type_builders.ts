import { AlgebraicType, ProductTypeElement, ScheduleAt, SumTypeVariant, type AlgebraicTypeVariants } from "..";
import type __AlgebraicType from "../autogen/algebraic_type_type";
import type { Prettify } from "./type_util";

/**
 * A set of methods for building a column definition. Type builders extend this
 * interface so that they can be converted to column builders by calling
 * one of the methods that returns a column builder.
 */
interface IntoColumnBuilder<Type, SpacetimeType extends AlgebraicType> {
  /**
   * Specify the index type for this column
   * @param algorithm The index algorithm to use
   */
  index<M extends ColumnMetadata = DefaultMetadata, N extends IndexTypes = "btree">(
    algorithm?: N
  ): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "indexType"> & { indexType: N }>>;

  /**
   * Specify this column as the primary key
   */
  primaryKey<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isPrimaryKey"> & { isPrimaryKey: true }>>;

  /**
   * Specify this column as unique
   */
  unique<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isUnique"> & { isUnique: true }>>;

  /**
   * Specify this column as auto-incrementing
   */
  autoInc<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isAutoIncrement"> & { isAutoIncrement: true }>>;

  /**
   * Specify this column as a schedule-at field
   */
  scheduleAt<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isScheduleAt"> & { isScheduleAt: true }>>;
}

/**
 * Helper type to extract the TypeScript type from a TypeBuilder
 */
type InferTypeOfTypeBuilder<T> = T extends TypeBuilder<infer U, any> ? U : never;

/**
 * Helper type to extract the TypeScript type from a TypeBuilder
 */
export type Infer<T> = InferTypeOfTypeBuilder<T>;

/**
 * Helper type to extract the type of a row from an object.
 */
type InferTypeOfRow<T> = T extends Record<string, ColumnBuilder<infer U, any, any> | TypeBuilder<infer U, any>> ? { [K in keyof T]: T[K] extends ColumnBuilder<infer V, any, any> ? V : T[K] extends TypeBuilder<infer V, any> ? V : never } : never;

/**
 * Type which represents a valid argument to the ProductColumnBuilder
 */
type ElementsObj = Record<string, TypeBuilder<any, any>>;

/**
 * Type which converts the elements of ElementsObj to a ProductType elements array
 */
type ElementsArrayFromElementsObj<Obj extends ElementsObj> = { name: keyof Obj & string; algebraicType: Obj[keyof Obj & string]["spacetimeType"] }[];

/**
 * A type which converts the elements of ElementsObj to a TypeScript object type.
 * It works by `Infer`ing the types of the column builders which are the values of
 * the keys in the object passed in.
 *
 * e.g. { a: I32TypeBuilder, b: StringBuilder } -> { a: number, b: string }
 */
type TypeScriptTypeFromElementsObj<Elements extends ElementsObj> = {
  [K in keyof Elements]: InferTypeOfTypeBuilder<Elements[K]>
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
  [K in keyof Variants]: { tag: K; value: InferTypeOfTypeBuilder<Variants[K]> }
}[keyof Variants];

/**
 * Type which converts the elements of VariantsObj to a SumType variants array
 */
type VariantsArrayFromVariantsObj<Obj extends VariantsObj> = { name: keyof Obj & string; algebraicType: Obj[keyof Obj & string]["spacetimeType"] }[];

/**
 * A generic type builder that captures both the TypeScript type
 * and the corresponding `AlgebraicType`.
 */
export class TypeBuilder<Type, SpacetimeType extends AlgebraicType> implements IntoColumnBuilder<Type, SpacetimeType> {
  /**
   * The TypeScript phantom type. This is not stored at runtime,
   * but is visible to the compiler
   */
  readonly type!: Type;
 
  /**
   * TypeScript phantom type representing the type of the particular
   * AlgebraicType stored in {@link algebraicType}.
   */
  readonly spacetimeType!: SpacetimeType;

  /**
   * The SpacetimeDB algebraic type (run‑time value). In addition to storing
   * the runtime representation of the `AlgebraicType`, it also captures
   * the TypeScript type information of the `AlgebraicType`. That is to say
   * the value is not merely an `AlgebraicType`, but is constructed to be
   * the corresponding concrete `AlgebraicType` for the TypeScript type `Type`.
   *
   * e.g. `string` corresponds to `AlgebraicType.String`
   */
  readonly algebraicType: AlgebraicType;

  constructor(algebraicType: AlgebraicType) {
    this.algebraicType = algebraicType;
  }

  /**
   * Specify the index type for this column
   * @param algorithm The index algorithm to use
   */
  index<M extends ColumnMetadata = DefaultMetadata, N extends IndexTypes = "btree">(
    algorithm?: N
  ): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "indexType"> & { indexType: N }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "indexType"> & { indexType: N }>>(this, {
      ...defaultMetadata,
      indexType: algorithm
    });
  }

  /**
   * Specify this column as the primary key
   */
  primaryKey<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isPrimaryKey"> & { isPrimaryKey: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isPrimaryKey"> & { isPrimaryKey: true }>>(this, {
      ...defaultMetadata,
      isPrimaryKey: true
    });
  }

  /**
   * Specify this column as unique
   */
  unique<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isUnique"> & { isUnique: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isUnique"> & { isUnique: true }>>(this, {
      ...defaultMetadata,
      isUnique: true
    });
  }

  /**
   * Specify this column as auto-incrementing
   */
  autoInc<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isAutoIncrement"> & { isAutoIncrement: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isAutoIncrement"> & { isAutoIncrement: true }>>(this, {
      ...defaultMetadata,
      isAutoIncrement: true
    });
  }

  /**
   * Specify this column as a schedule-at field
   */
  scheduleAt<M extends ColumnMetadata = DefaultMetadata>(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isScheduleAt"> & { isScheduleAt: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isScheduleAt"> & { isScheduleAt: true }>>(this, {
      ...defaultMetadata,
      isScheduleAt: true
    });
  }
}

export class U8Builder extends TypeBuilder<number, AlgebraicTypeVariants.U8> {
  constructor() {
    super(AlgebraicType.U8);
  }
}
export class U16Builder extends TypeBuilder<number, AlgebraicTypeVariants.U16> {
  constructor() {
    super(AlgebraicType.U16);
  }
}
export class U32Builder extends TypeBuilder<number, AlgebraicTypeVariants.U32> {
  constructor() {
    super(AlgebraicType.U32);
  }
}
export class U64Builder extends TypeBuilder<bigint, AlgebraicTypeVariants.U64> {
  constructor() {
    super(AlgebraicType.U64);
  }
}
export class U128Builder extends TypeBuilder<bigint, AlgebraicTypeVariants.U128> {
  constructor() {
    super(AlgebraicType.U128);
  }
}
export class U256Builder extends TypeBuilder<bigint, AlgebraicTypeVariants.U256> {
  constructor() {
    super(AlgebraicType.U256);
  }
}
export class I128Builder extends TypeBuilder<bigint, AlgebraicTypeVariants.I128> {
  constructor() {
    super(AlgebraicType.I128);
  }
}
export class I256Builder extends TypeBuilder<bigint, AlgebraicTypeVariants.I256> {
  constructor() {
    super(AlgebraicType.I256);
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
export class BoolBuilder extends TypeBuilder<boolean, AlgebraicTypeVariants.Bool> {
  constructor() {
    super(AlgebraicType.Bool);
  }
}
export class StringBuilder extends TypeBuilder<string, AlgebraicTypeVariants.String> {
  constructor() {
    super(AlgebraicType.String);
  }
}
export class I8Builder extends TypeBuilder<number, AlgebraicTypeVariants.I8> {
  constructor() {
    super(AlgebraicType.I8);
  }
}
export class I16Builder extends TypeBuilder<number, AlgebraicTypeVariants.I16> {
  constructor() {
    super(AlgebraicType.I16);
  }
}
export class I32Builder extends TypeBuilder<number, AlgebraicTypeVariants.I32> {
  constructor() {
    super(AlgebraicType.I32);
  }
}
export class I64Builder extends TypeBuilder<bigint, AlgebraicTypeVariants.I64> {
  constructor() {
    super(AlgebraicType.I64);
  }
}
export class ArrayBuilder<Element extends TypeBuilder<any, any>> extends TypeBuilder<Array<Element["type"]>, { tag: "Array", value: Element["spacetimeType"] }> {
  /**
   * The phantom element type of the array for TypeScript
   */
  readonly element!: Element;

  constructor(element: Element) {
    super(AlgebraicType.Array(element.algebraicType));
  }
}
export class ProductBuilder<Elements extends ElementsObj> extends TypeBuilder<TypeScriptTypeFromElementsObj<Elements>, { tag: "Product", value: { elements: ElementsArrayFromElementsObj<Elements> } }> {
  /**
   * The phantom element types of the product for TypeScript
   */
  readonly elements!: Elements;

  constructor(elements: Elements) {
    function elementsArrayFromElementsObj<Obj extends ElementsObj>(obj: Obj) {
      return Object.entries(obj).map(([name, { algebraicType }]) => ({ name, algebraicType }));
    }
    super(AlgebraicType.Product({
      elements: elementsArrayFromElementsObj(elements)
    }));
  }
}
export class SumBuilder<Variants extends VariantsObj> extends TypeBuilder<TypeScriptTypeFromVariantsObj<Variants>, { tag: "Sum", value: { variants: VariantsArrayFromVariantsObj<Variants> } }> {
  /**
   * The phantom variant types of the sum for TypeScript
   */
  readonly variants!: Variants;

  constructor(variants: Variants) {
    function variantsArrayFromVariantsObj<Variants extends VariantsObj>(variants: Variants): SumTypeVariant[] {
      return Object.entries(variants).map(([name, { algebraicType }]) => ({ name, algebraicType }));
    }
    super(AlgebraicType.Sum({
      variants: variantsArrayFromVariantsObj(variants)
    }));
  }
}

export interface U8ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.U8> { }
export interface U16ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.U16> { }
export interface U32ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.U32> { }
export interface U64ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.U64> { }
export interface U128ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.U128> { }
export interface U256ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.U256> { }
export interface I8ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.I8> { }
export interface I16ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.I16> { }
export interface I32ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.I32> { }
export interface I64ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.I64> { }
export interface I128ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.I128> { }
export interface I256ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.I256> { }
export interface F32ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.F32> { }
export interface F64ColumnBuilder extends ColumnBuilder<number, AlgebraicTypeVariants.F64> { }
export interface BoolColumnBuilder extends ColumnBuilder<boolean, AlgebraicTypeVariants.Bool> { }
export interface StringColumnBuilder extends ColumnBuilder<string, AlgebraicTypeVariants.String> { }
export interface ArrayColumnBuilder<Element extends TypeBuilder<any, any>> extends ColumnBuilder<Array<Element["type"]>, { tag: "Array", value: Element["spacetimeType"] }> { }
export interface ProductColumnBuilder<Elements extends Array<{ name: string, algebraicType: AlgebraicType }>> extends ColumnBuilder<{ [K in Elements[number]['name']]: any }, { tag: "Product", value: { elements: Elements } }> { }
export interface SumColumnBuilder<Variants extends Array<{ name: string, algebraicType: AlgebraicType }>> extends ColumnBuilder<{ [K in Variants[number]['name']]: { tag: K; value: any } }[Variants[number]['name']], { tag: "Sum", value: { variants: Variants } }> { }

/**
 * The type of index types that can be applied to a column.
 * `undefined` is the default
 */
type IndexTypes = "btree" | "hash" | undefined;

/**
 * Metadata describing column constraints and index type
 */
export type ColumnMetadata = {
  isPrimaryKey: boolean;
  isUnique: boolean;
  isAutoIncrement: boolean;
  isScheduleAt: boolean;
  indexType: IndexTypes;
};

/**
 * Default metadata state type for a newly created column
 */
type DefaultMetadata = {
  isPrimaryKey: false;
  isUnique: false;
  isAutoIncrement: false;
  isScheduleAt: false;
  indexType: undefined;
};

/**
 * Default metadata state value for a newly created column
 */
const defaultMetadata: DefaultMetadata = {
  isPrimaryKey: false,
  isUnique: false,
  isAutoIncrement: false,
  isScheduleAt: false,
  indexType: undefined
};

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
  M extends ColumnMetadata = DefaultMetadata
> implements IntoColumnBuilder<Type, SpacetimeType> {
  typeBuilder: TypeBuilder<Type, SpacetimeType>;
  columnMetadata: ColumnMetadata;

  constructor(typeBuilder: TypeBuilder<Type, SpacetimeType>, metadata?: ColumnMetadata) {
    this.typeBuilder = typeBuilder;
    this.columnMetadata = defaultMetadata;
  }

  /**
   * Specify the index type for this column
   * @param algorithm The index algorithm to use
   */
  index<N extends IndexTypes = "btree">(
    algorithm?: N
  ): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "indexType"> & { indexType: N }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "indexType"> & { indexType: N }>>(
      this.typeBuilder,
      {
        ...this.columnMetadata,
        indexType: algorithm
      }
    );
  }

  /**
   * Specify this column as the primary key
   */
  primaryKey(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isPrimaryKey"> & { isPrimaryKey: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isPrimaryKey"> & { isPrimaryKey: true }>>(
      this.typeBuilder,
      {
        ...this.columnMetadata,
        isPrimaryKey: true
      }
    );
  }

  /**
   * Specify this column as unique
   */
  unique(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isUnique"> & { isUnique: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isUnique"> & { isUnique: true }>>(
      this.typeBuilder,
      {
        ...this.columnMetadata,
        isUnique: true
      }
    );
  }

  /**
   * Specify this column as auto-incrementing
   */
  autoInc(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isAutoIncrement"> & { isAutoIncrement: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isAutoIncrement"> & { isAutoIncrement: true }>>(
      this.typeBuilder,
      {
        ...this.columnMetadata,
        isAutoIncrement: true
      }
    );
  }

  /**
   * Specify this column as a schedule-at field
   */
  scheduleAt(): ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isScheduleAt"> & { isScheduleAt: true }>> {
    return new ColumnBuilder<Type, SpacetimeType, Prettify<Omit<M, "isScheduleAt"> & { isScheduleAt: true }>>(
      this.typeBuilder,
      {
        ...this.columnMetadata,
        isScheduleAt: true
      }
    );
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
const t = {
  /**
   * Creates a new `Bool` {@link AlgebraicType} to be used in table definitions
   * Represented as `boolean` in TypeScript.
   * @returns A new BoolBuilder instance
   */
  bool: (): BoolBuilder => new BoolBuilder(),

  /**
   * Creates a new `String` {@link AlgebraicType} to be used in table definitions
   * Represented as `string` in TypeScript.
   * @returns A new StringBuilder instance
   */
  string: (): StringBuilder => new StringBuilder(),

  /**
   * Creates a new `F64` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new F64Builder instance
   */
  number: (): F64Builder => new F64Builder(),

  /**
   * Creates a new `I8` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new I8Builder instance
   */
  i8: (): I8Builder => new I8Builder(),

  /**
   * Creates a new `U8` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new U8Builder instance
   */
  u8: (): U8Builder => new U8Builder(),

  /**
   * Creates a new `I16` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new I16Builder instance
   */
  i16: (): I16Builder => new I16Builder(),

  /**
   * Creates a new `U16` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new U16Builder instance
   */
  u16: (): U16Builder => new U16Builder(),

  /**
   * Creates a new `I32` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new I32Builder instance
   */
  i32: (): I32Builder => new I32Builder(),

  /**
   * Creates a new `U32` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new U32Builder instance
   */
  u32: (): U32Builder => new U32Builder(),

  /**
   * Creates a new `I64` AlgebraicType to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new U32Builder instance
   */
  i64: (): I64Builder => new I64Builder(),

  /**
   * Creates a new `U64` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new U64Builder instance
   */
  u64: (): U64Builder => new U64Builder(),

  /**
   * Creates a new `I128` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new I128Builder instance
   */
  i128: (): I128Builder => new I128Builder(),

  /**
   * Creates a new `U128` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new U128Builder instance
   */
  u128: (): U128Builder => new U128Builder(),

  /**
   * Creates a new `I256` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new I256Builder instance
   */
  i256: (): I256Builder => new I256Builder(),

  /**
   * Creates a new `U256` {@link AlgebraicType} to be used in table definitions
   * Represented as `bigint` in TypeScript.
   * @returns A new U256Builder instance
   */
  u256: (): U256Builder => new U256Builder(),

  /**
   * Creates a new `F32` AlgebraicType to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new F32Builder instance
   */
  f32: (): F32Builder => new F32Builder(),

  /**
   * Creates a new `F64` {@link AlgebraicType} to be used in table definitions
   * Represented as `number` in TypeScript.
   * @returns A new F64Builder instance
   */
  f64: (): F64Builder => new F64Builder(),

  /**
   * Creates a new `Object` AlgebraicType to be used in table definitions.
   * Properties of the object must also be {@link TypeBuilder}s.
   * Represented as an object with specific properties in TypeScript.
   * @param obj The object defining the properties of the type, whose property
   * values must be {@link TypeBuilder}s.
   * @returns A new ObjectBuilder instance
   */
  object<Obj extends ElementsObj>(
    obj: Obj
  ): ProductBuilder<Obj> {
    return new ProductBuilder<Obj>(obj);
  },

  /**
   * Creates a new `Array` AlgebraicType to be used in table definitions.
   * Represented as an array in TypeScript.
   * @param element The element type of the array, which must be a `TypeBuilder`.
   * @returns A new ArrayBuilder instance
   */
  array<Element extends TypeBuilder<any, any>>(e: Element): ArrayBuilder<Element> {
    return new ArrayBuilder<Element>(e);
  },

  /**
   * Creates a new `Enum` AlgebraicType to be used in table definitions.
   * Represented as a union of string literals in TypeScript.
   * @param obj The object defining the variants of the enum, whose variant
   * types must be `TypeBuilder`s.
   * @returns A new EnumBuilder instance
   */
  enum<Obj extends VariantsObj>(
    obj: Obj
  ): SumBuilder<Obj> {
    return new SumBuilder<Obj>(obj);
  },

  /**
   * This is a special helper function for conveniently creating {@link ScheduleAt} type columns.
   * @returns A new ColumnBuilder instance with the {@link ScheduleAt} type.
   */
  scheduleAt: (): ColumnBuilder<ScheduleAt, ReturnType<typeof ScheduleAt.getAlgebraicType>, Omit<ColumnMetadata, "isScheduleAt"> & { isScheduleAt: true }> => {
    return new ColumnBuilder<ScheduleAt, ReturnType<typeof ScheduleAt.getAlgebraicType>, Omit<ColumnMetadata, "isScheduleAt"> & { isScheduleAt: true }>(
      new TypeBuilder<ScheduleAt, ReturnType<typeof ScheduleAt.getAlgebraicType>>(ScheduleAt.getAlgebraicType()),
      {
        ...defaultMetadata,
        isScheduleAt: true
      }
    );
  },
} as const;
export default t;

// @typescript-eslint/no-unused-vars
namespace tests {
  type MustBeNever<T> = [T] extends [never] 
    ? true 
    : ["Error: Type must be never", T];

  // Test type inference on a row
  // i.e. a Record<string, TypeBuilder | ColumnBuilder> type
  const row = {
    foo: t.string(),
    bar: t.i32().primaryKey(),
    idx: t.i64().index("btree").unique()
  };
  type Row = InferTypeOfRow<typeof row>;
  const _row: Row = {
    foo: "hello",
    bar: 42,
    idx: 100n
  };

  // Test that a row must not allow non-TypeBuilder or ColumnBuilder values
  const row2 = {
    foo: {
      // bar is not a TypeBuilder or ColumnBuilder, so this should fail
      bar: t.string()
    },
    bar: t.i32().primaryKey(),
    idx: t.i64().index("btree").unique()
  };
  type Row2 = InferTypeOfRow<typeof row2>;
  type _ = MustBeNever<Row2>;

  // Test type inference on a type with a nested object
  const point = t.object({
    x: t.i32(),
    y: t.f64(),
    z: t.object({
      foo: t.string()
    })
  });
  type Point = InferTypeOfTypeBuilder<typeof point>;
  const _point: Point = {
    x: 1.0,
    y: 2.0,
    z: {
      foo: "bar"
     }
  };

  // Test type inference on an enum
  const e = t.enum({
    A: t.string(),
    B: t.number()
  });
  type E = InferTypeOfTypeBuilder<typeof e>;
  const _e: E = { tag: "A", value: "hello" };
  const _e2: E = { tag: "B", value: 42 };

  // Test that the type of a row includes the correct ColumnBuilder types
  const _row3: {
    foo: TypeBuilder<string, AlgebraicTypeVariants.String>;
    bar: ColumnBuilder<number, AlgebraicTypeVariants.I32, { isPrimaryKey: true; isUnique: false; isAutoIncrement: false; isScheduleAt: false; indexType: undefined; }>;
    idx: ColumnBuilder<bigint, AlgebraicTypeVariants.I64, { isPrimaryKey: false; isUnique: true; isAutoIncrement: false; isScheduleAt: false; indexType: "btree"; }>;
  } = {
    foo: t.string(),
    bar: t.i32().primaryKey(),
    idx: t.i64().index("btree").unique()
  };
}
