import {
  AlgebraicType,
  ProductType,
  type AlgebraicTypeVariants,
  type Deserializer,
  type Serializer,
} from '../lib/algebraic_type';
import type { Identity } from '../lib/identity';
import type { OptionAlgebraicType } from '../lib/option';
import type { ParamsObj } from '../lib/reducers';
import { type UntypedSchemaDef } from '../lib/schema';
import {
  ArrayBuilder,
  OptionBuilder,
  RowBuilder,
  type ColumnBuilder,
  type ColumnMetadata,
  type Infer,
  type InferSpacetimeTypeOfTypeBuilder,
  type InferTypeOfRow,
  type RowObj,
  type TypeBuilder,
} from '../lib/type_builders';
import type { IsUnion } from '../lib/type_util';
import { bsatnBaseSize, toPascalCase } from '../lib/util';
import type { ReadonlyDbView } from './db_view';
import { type QueryBuilder, type RowTypedQuery } from './query';
import {
  exportContext,
  registerExport,
  type ModuleExport,
  type SchemaInner,
} from './schema';

export type ViewExport<ViewFn> = ViewFn & ModuleExport;

export function makeViewExport<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
  F extends ViewFn<S, Params, Ret>,
>(
  ctx: SchemaInner,
  opts: ViewOpts,
  params: Params,
  ret: Ret,
  fn: F
): ViewExport<F> {
  const viewExport =
    // @ts-expect-error typescript incorrectly says Function#bind requires an argument.
    fn.bind() as ViewExport<F>;
  viewExport[exportContext] = ctx;
  viewExport[registerExport] = (ctx, exportName) => {
    registerView(ctx, opts, exportName, false, params, ret, fn);
  };
  return viewExport;
}

export function makeAnonViewExport<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
  F extends AnonymousViewFn<S, Params, Ret>,
>(
  ctx: SchemaInner,
  opts: ViewOpts,
  params: Params,
  ret: Ret,
  fn: F
): ViewExport<F> {
  const viewExport =
    // @ts-expect-error typescript incorrectly says Function#bind requires an argument.
    fn.bind() as ViewExport<F>;
  viewExport[exportContext] = ctx;
  viewExport[registerExport] = (ctx, exportName) => {
    registerView(ctx, opts, exportName, true, params, ret, fn);
  };
  return viewExport;
}

export type ViewCtx<S extends UntypedSchemaDef> = Readonly<{
  sender: Identity;
  db: ReadonlyDbView<S>;
  from: QueryBuilder<S>;
}>;

export type AnonymousViewCtx<S extends UntypedSchemaDef> = Readonly<{
  db: ReadonlyDbView<S>;
  from: QueryBuilder<S>;
}>;

export type ViewOpts = {
  name?: string;
  public: true;
};

type FlattenedArray<T> = T extends readonly (infer E)[] ? E : never;

// Compile-time mirror of `viewReturnRow` below. Views currently return either
// `array(row(...))` or `option(row(...))`; this extracts the row object type
// from those builders so we can inspect column metadata at the type level.
// Non-row returns collapse to `never`, which makes the primary-key validation
// below a no-op for unsupported shapes.
type ViewReturnRow<Ret extends ViewReturnTypeBuilder> =
  Ret extends ArrayBuilder<infer Element>
    ? Element extends RowBuilder<infer Row>
      ? Row
      : never
    : Ret extends OptionBuilder<infer Value>
      ? Value extends RowBuilder<infer Row>
        ? Row
        : never
      : never;

// Produces a union of the returned row's column names marked with
// `.primaryKey()`. For example, `{ id: t.u32().primaryKey(), name: t.string() }`
// becomes `"id"`, while two marked columns becomes `"id" | "name"`.
type PrimaryKeyColumnNames<Row extends RowObj> = {
  [K in keyof Row & string]: Row[K] extends ColumnBuilder<any, any, infer M>
    ? M extends { isPrimaryKey: true }
      ? K
      : never
    : never;
}[keyof Row & string];

// In generic code, row keys may widen from literal names like "id" | "name"
// to plain `string`. That means "unknown column name", not "multiple primary
// keys", so avoid a false-positive type error and rely on the runtime check.
type HasMultiplePrimaryKeys<Row extends RowObj> =
  string extends PrimaryKeyColumnNames<Row>
    ? false
    : IsUnion<PrimaryKeyColumnNames<Row>>;

type MultiplePrimaryKeyColumns<Ret extends ViewReturnTypeBuilder> =
  PrimaryKeyColumnNames<ViewReturnRow<Ret>>;

type ERROR_view_return_type_can_have_at_most_one_primaryKey<
  Columns extends string,
> = {
  _primaryKeyColumns: Columns;
  _fix: 'Remove primaryKey() from all but one column on the returned row type';
};

// Used as a rest parameter type on `Schema.view` and `Schema.anonymousView`.
// Valid return builders produce `[]`, so callers pass no extra arguments. If
// the returned row has multiple `.primaryKey()` columns, this becomes a
// one-element tuple containing an explanatory error type, which makes the
// normal three-argument call fail to type-check.
export type ValidateViewPrimaryKey<Ret extends ViewReturnTypeBuilder> =
  HasMultiplePrimaryKeys<ViewReturnRow<Ret>> extends true
    ? [
        error: ERROR_view_return_type_can_have_at_most_one_primaryKey<
          MultiplePrimaryKeyColumns<Ret>
        >,
      ]
    : [];

// // If we allowed functions to return either.
// type ViewReturn<Ret extends ViewReturnTypeBuilder> =
//   | Infer<Ret>
//   | RowTypedQuery<FlattenedArray<Infer<Ret>>>;

export type ViewFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
> =
  | ((ctx: ViewCtx<S>, params: InferTypeOfRow<Params>) => Infer<Ret>)
  | ((
      ctx: ViewCtx<S>,
      params: InferTypeOfRow<Params>
    ) => RowTypedQuery<FlattenedArray<Infer<Ret>>, ExtractArrayProduct<Ret>>);

export type AnonymousViewFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
> =
  | ((ctx: AnonymousViewCtx<S>, params: InferTypeOfRow<Params>) => Infer<Ret>)
  | ((
      ctx: AnonymousViewCtx<S>,
      params: InferTypeOfRow<Params>
    ) => RowTypedQuery<FlattenedArray<Infer<Ret>>, ExtractArrayProduct<Ret>>);

export type ViewReturnTypeBuilder =
  | TypeBuilder<
      readonly object[],
      { tag: 'Array'; value: AlgebraicTypeVariants.Product }
    >
  | TypeBuilder<
      object | undefined,
      OptionAlgebraicType<AlgebraicTypeVariants.Product>
    >;

export function registerView<
  S extends UntypedSchemaDef,
  const Anonymous extends boolean,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
>(
  ctx: SchemaInner,
  opts: ViewOpts,
  exportName: string,
  anon: Anonymous,
  params: Params,
  ret: Ret,
  fn: Anonymous extends true
    ? AnonymousViewFn<S, Params, Ret>
    : ViewFn<S, Params, Ret>
) {
  ctx.defineFunction(exportName);
  const paramsBuilder = new RowBuilder(params, toPascalCase(exportName));

  // Register return types if they are product types
  let returnType = ctx.registerTypesRecursively(ret).algebraicType;

  const { typespace } = ctx;

  const { value: paramType } = ctx.resolveType(
    ctx.registerTypesRecursively(paramsBuilder)
  );

  ctx.moduleDef.views.push({
    sourceName: exportName,
    index: (anon ? ctx.anonViews : ctx.views).length,
    isPublic: opts.public,
    isAnonymous: anon,
    params: paramType,
    returnType,
  });

  // Runtime counterpart to `ValidateViewPrimaryKey`: the type-level check gives
  // users an early diagnostic in normal code, but this still protects dynamic
  // or widened builders and is the source of the raw module-def metadata.
  const primaryKeyColumns = viewPrimaryKeyColumns(ret);
  if (primaryKeyColumns.length > 1) {
    throw new TypeError(
      `View '${exportName}' can have at most one primaryKey() column on its returned row type; found ${primaryKeyColumns.join(', ')}`
    );
  }
  if (primaryKeyColumns.length === 1) {
    ctx.moduleDef.viewPrimaryKeys.push({
      viewSourceName: exportName,
      columns: primaryKeyColumns,
    });
  }

  if (opts.name != null) {
    ctx.moduleDef.explicitNames.entries.push({
      tag: 'Function',
      value: {
        sourceName: exportName,
        canonicalName: opts.name,
      },
    });
  }

  // If it is an option, we wrap the function to make the return look like an array.
  if (returnType.tag == 'Sum') {
    const originalFn = fn;
    fn = ((ctx: ViewCtx<S>, args: InferTypeOfRow<Params>) => {
      const ret = originalFn(ctx, args);
      return ret == null ? [] : [ret];
    }) as any;
    returnType = AlgebraicType.Array(
      returnType.value.variants[0].algebraicType
    );
  }

  (anon ? ctx.anonViews : ctx.views).push({
    fn,
    deserializeParams: ProductType.makeDeserializer(paramType, typespace),
    serializeReturn: AlgebraicType.makeSerializer(returnType, typespace),
    returnTypeBaseSize: bsatnBaseSize(typespace, returnType),
  });
}

// Inspect the returned row builder and collect the column property names marked
// with `.primaryKey()`. These names are the TypeScript row-builder keys, which
// are also the raw column names in the module definition emitted by the TS SDK.
function viewPrimaryKeyColumns(ret: ViewReturnTypeBuilder): string[] {
  const row = viewReturnRow(ret);
  if (row == null) {
    return [];
  }

  return Object.entries(row.row)
    .filter(
      (
        entry
      ): entry is [string, ColumnBuilder<any, any, ColumnMetadata<any>>] =>
        entry[1].columnMetadata.isPrimaryKey === true
    )
    .map(([name]) => name);
}

// Views can return either `array(row(...))` or `option(row(...))`. The primary
// key marker lives on the inner `RowBuilder`, so unwrap those two supported
// shapes and ignore anything else.
function viewReturnRow(
  ret: ViewReturnTypeBuilder
): RowBuilder<any> | undefined {
  if (ret instanceof ArrayBuilder && ret.element instanceof RowBuilder) {
    return ret.element;
  }
  if (ret instanceof OptionBuilder && ret.value instanceof RowBuilder) {
    return ret.value;
  }
  return undefined;
}

type ViewInfo<F> = {
  fn: F;
  deserializeParams: Deserializer<any>;
  serializeReturn: Serializer<any>;
  returnTypeBaseSize: number;
};

type AnyViewFn = (ctx: ViewCtx<any>, params: any) => any;
type AnyAnonymousViewFn = (ctx: AnonymousViewCtx<any>, params: any) => any;

export type Views = ViewInfo<AnyViewFn>[];
export type AnonViews = ViewInfo<AnyAnonymousViewFn>[];

// A helper to get the product type out of a type builder.
// This is only non-never if the type builder is an array.
type ExtractArrayProduct<T extends TypeBuilder<any, any>> =
  InferSpacetimeTypeOfTypeBuilder<T> extends { tag: 'Array'; value: infer V }
    ? V extends { tag: 'Product'; value: infer P }
      ? P
      : never
    : never;
