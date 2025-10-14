import { t } from '../server';
import { type AlgebraicTypeVariants } from '..';
import type {
  I32ColumnBuilder,
  I64ColumnBuilder,
  InferTypeOfRow,
  InferTypeOfTypeBuilder,
  TypeBuilder,
  U8ColumnBuilder,
} from './type_builders';

type MustBeNever<T> = [T] extends [never]
  ? true
  : ['Error: Type must be never', T];

// Test type inference on a row
// i.e. a Record<string, TypeBuilder | ColumnBuilder> type
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const row = {
  foo: t.string(),
  bar: t.i32().primaryKey(),
  idx: t.i64().index('btree').unique(),
};
type Row = InferTypeOfRow<typeof row>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _row: Row = {
  foo: 'hello',
  bar: 42,
  idx: 100n,
};

// eslint-disable-next-line @typescript-eslint/no-unused-vars
const rowOptionOptional = {
  foo: t.string().optional().optional(),
};
type RowOptionOptional = InferTypeOfRow<typeof rowOptionOptional>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _rowOptionOptionalNone: RowOptionOptional = {
  foo: undefined,
};
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _rowOptionOptionalSome: RowOptionOptional = {
  foo: 'hello',
};

// Test that a row must not allow non-TypeBuilder or ColumnBuilder values
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const row2 = {
  foo: {
    // bar is not a TypeBuilder or ColumnBuilder, so this should fail
    bar: t.string(),
  },
  bar: t.i32().primaryKey(),
  idx: t.i64().index('btree').unique(),
};
// @ts-expect-error this should error
type Row2 = InferTypeOfRow<typeof row2>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
type _ = MustBeNever<Row2>;

// Test type inference on a type with a nested object
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const point = t.object('Point', {
  x: t.i32(),
  y: t.f64(),
  z: t.object('Foo', {
    foo: t.string(),
  }),
});
type Point = InferTypeOfTypeBuilder<typeof point>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _point: Point = {
  x: 1.0,
  y: 2.0,
  z: {
    foo: 'bar',
  },
};

// Test type inference on an enum
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const e = t.enum('E', {
  A: t.string(),
  B: t.number(),
});
type E = InferTypeOfTypeBuilder<typeof e>;
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _e: E = { tag: 'A', value: 'hello' };
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _e2: E = { tag: 'B', value: 42 };

// Test that the type of a row includes the correct ColumnBuilder types
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _row3: {
  foo: TypeBuilder<string, AlgebraicTypeVariants.String>;
  bar: I32ColumnBuilder<{
    isPrimaryKey: true;
  }>;
  idx: I64ColumnBuilder<{
    isUnique: true;
    indexType: 'btree';
  }>;
} = {
  foo: t.string(),
  bar: t.i32().primaryKey(),
  idx: t.i64().index('btree').unique(),
};

// Test that you can add the index and unique constraint in any order
// eslint-disable-next-line @typescript-eslint/no-unused-vars
const _row4: {
  foo: TypeBuilder<string, AlgebraicTypeVariants.String>;
  baz: U8ColumnBuilder<{
    isAutoIncrement: true;
  }>;
  bar: I32ColumnBuilder<{
    isPrimaryKey: true;
  }>;
  idx: I64ColumnBuilder<{
    isUnique: true;
    indexType: 'btree';
  }>;
} = {
  foo: t.string(),
  baz: t.u8().autoInc(),
  bar: t.i32().primaryKey(),
  idx: t.i64().unique().index('btree'),
};
