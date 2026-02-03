import {
  AlgebraicType,
  ProductType,
  type AlgebraicTypeVariants,
  type Deserializer,
  type Serializer,
} from '../lib/algebraic_type';
import type { Identity } from '../lib/identity';
import type { OptionAlgebraicType } from '../lib/option';
import type { ParamsObj } from './reducers';
import {
  MODULE_DEF,
  registerTypesRecursively,
  resolveType,
  type UntypedSchemaDef,
} from './schema';
import type { ReadonlyTable } from './table';
import {
  RowBuilder,
  type Infer,
  type InferSpacetimeTypeOfTypeBuilder,
  type InferTypeOfRow,
  type TypeBuilder,
} from './type_builders';
import { bsatnBaseSize, toPascalCase } from './util';
import { type QueryBuilder, type RowTypedQuery } from './query';

export type ViewCtx<S extends UntypedSchemaDef> = Readonly<{
  sender: Identity;
  db: ReadonlyDbView<S>;
  from: QueryBuilder<S>;
}>;

export type AnonymousViewCtx<S extends UntypedSchemaDef> = Readonly<{
  db: ReadonlyDbView<S>;
  from: QueryBuilder<S>;
}>;

export type ReadonlyDbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: ReadonlyTable<Tbl>;
};

export type ViewOpts = {
  name: string;
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

export function defineView<
  S extends UntypedSchemaDef,
  const Anonymous extends boolean,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
>(
  opts: ViewOpts,
  anon: Anonymous,
  params: Params,
  ret: Ret,
  fn: Anonymous extends true
    ? AnonymousViewFn<S, Params, Ret>
    : ViewFn<S, Params, Ret>
) {
  const paramsBuilder = new RowBuilder(params, toPascalCase(opts.name));

  // Register return types if they are product types
  let returnType = registerTypesRecursively(ret).algebraicType;

  const { typespace } = MODULE_DEF;

  const { value: paramType } = resolveType(
    typespace,
    registerTypesRecursively(paramsBuilder)
  );

  MODULE_DEF.miscExports.push({
    tag: 'View',
    value: {
      name: opts.name,
      index: (anon ? ANON_VIEWS : VIEWS).length,
      isPublic: opts.public,
      isAnonymous: anon,
      params: paramType,
      returnType,
    },
  });

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

  (anon ? ANON_VIEWS : VIEWS).push({
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

export const VIEWS: ViewInfo<ViewFn<any, any, any>>[] = [];
export const ANON_VIEWS: ViewInfo<AnonymousViewFn<any, any, any>>[] = [];

// A helper to get the product type out of a type builder.
// This is only non-never if the type builder is an array.
type ExtractArrayProduct<T extends TypeBuilder<any, any>> =
  InferSpacetimeTypeOfTypeBuilder<T> extends { tag: 'Array'; value: infer V }
    ? V extends { tag: 'Product'; value: infer P }
      ? P
      : never
    : never;
