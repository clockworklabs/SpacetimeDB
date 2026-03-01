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
  RowBuilder,
  type Infer,
  type InferSpacetimeTypeOfTypeBuilder,
  type InferTypeOfRow,
  type TypeBuilder,
} from '../lib/type_builders';
import { bsatnBaseSize, toPascalCase } from '../lib/util';
import type { ReadonlyDbView } from './db_view';
import {
  isRowTypedQuery,
  type QueryBuilder,
  type RowTypedQuery,
} from './query';
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
      return isRowTypedQuery(ret) ? ret : ret == null ? [] : [ret];
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

type ViewInfo<F> = {
  fn: F;
  deserializeParams: Deserializer<any>;
  serializeReturn: Serializer<any>;
  returnTypeBaseSize: number;
};

export type Views = ViewInfo<ViewFn<any, any, any>>[];
export type AnonViews = ViewInfo<AnonymousViewFn<any, any, any>>[];

// A helper to get the product type out of a type builder.
// This is only non-never if the type builder is an array.
type ExtractArrayProduct<T extends TypeBuilder<any, any>> =
  InferSpacetimeTypeOfTypeBuilder<T> extends { tag: 'Array'; value: infer V }
    ? V extends { tag: 'Product'; value: infer P }
      ? P
      : never
    : never;
