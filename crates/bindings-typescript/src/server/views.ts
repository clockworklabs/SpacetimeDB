import type {
  AlgebraicType,
  AlgebraicTypeVariants,
  ProductType,
} from '../lib/algebraic_type';
import type { Identity } from '../lib/identity';
import type { ParamsObj } from './reducers';
import { MODULE_DEF, type UntypedSchemaDef } from './schema';
import type { ReadonlyTable } from './table';
import type { Infer, InferTypeOfRow, TypeBuilder } from './type_builders';
import { bsatnBaseSize } from './util';

export type ViewCtx<S extends UntypedSchemaDef> = Readonly<{
  sender: Identity;
  db: ReadonlyDbView<S>;
}>;

export type AnonymousViewCtx<S extends UntypedSchemaDef> = Readonly<{
  db: ReadonlyDbView<S>;
}>;

export type ReadonlyDbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['name']]: ReadonlyTable<Tbl>;
};

export type ViewFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
> = (ctx: ViewCtx<S>, params: InferTypeOfRow<Params>) => Infer<Ret>;

export type AnonymousViewFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
> = (ctx: AnonymousViewCtx<S>, params: InferTypeOfRow<Params>) => Infer<Ret>;

export type ViewReturnTypeBuilder = TypeBuilder<
  any,
  | { tag: 'Array'; value: AlgebraicTypeVariants.Product }
  | AlgebraicTypeVariants.Product
>;

export function defineView<
  S extends UntypedSchemaDef,
  const Anonymous extends boolean,
  Params extends ParamsObj,
  Ret extends ViewReturnTypeBuilder,
>(
  name: string,
  anon: Anonymous,
  params: Params,
  ret: Ret,
  fn: Anonymous extends true
    ? AnonymousViewFn<S, Params, Ret>
    : ViewFn<S, Params, Ret>
) {
  const paramType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType: c.algebraicType,
    })),
  };

  MODULE_DEF.miscExports.push({
    tag: 'View',
    value: {
      name,
      index: (anon ? ANON_VIEWS : VIEWS).length,
      isPublic: true,
      isAnonymous: anon,
      params: paramType,
      returnType: ret.algebraicType,
    },
  });

  (anon ? ANON_VIEWS : VIEWS).push({
    fn,
    params: paramType,
    returnType: ret.algebraicType,
    returnTypeBaseSize: bsatnBaseSize(MODULE_DEF.typespace, ret.algebraicType),
  });
}

type ViewInfo<F> = {
  fn: F;
  params: ProductType;
  returnType: AlgebraicType;
  returnTypeBaseSize: number;
};

export const VIEWS: ViewInfo<ViewFn<any, any, any>>[] = [];
export const ANON_VIEWS: ViewInfo<AnonymousViewFn<any, any, any>>[] = [];
