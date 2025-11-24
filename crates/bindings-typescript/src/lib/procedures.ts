import { AlgebraicType, ProductType } from '../lib/algebraic_type';
import type { ConnectionId } from '../lib/connection_id';
import type { Identity } from '../lib/identity';
import type { Timestamp } from '../lib/timestamp';
import type { ParamsObj } from './reducers';
import {
  MODULE_DEF,
  registerTypesRecursively,
  type UntypedSchemaDef,
} from './schema';
import type { Infer, InferTypeOfRow, TypeBuilder } from './type_builders';
import { bsatnBaseSize } from './util';

export type ProcedureFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = (ctx: ProcedureCtx<S>, args: InferTypeOfRow<Params>) => Infer<Ret>;

// eslint-disable-next-line @typescript-eslint/no-unused-vars
export interface ProcedureCtx<S extends UntypedSchemaDef> {
  readonly sender: Identity;
  readonly identity: Identity;
  readonly timestamp: Timestamp;
  readonly connectionId: ConnectionId | null;
}

export function procedure<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
>(name: string, params: Params, ret: Ret, fn: ProcedureFn<S, Params, Ret>) {
  const paramsType: ProductType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType:
        'typeBuilder' in c ? c.typeBuilder.algebraicType : c.algebraicType,
    })),
  };
  const returnType = registerTypesRecursively(ret).algebraicType;

  MODULE_DEF.miscExports.push({
    tag: 'Procedure',
    value: {
      name,
      params: paramsType,
      returnType,
    },
  });

  PROCEDURES.push({
    fn,
    paramsType,
    returnType,
    returnTypeBaseSize: bsatnBaseSize(MODULE_DEF.typespace, returnType),
  });
}

export const PROCEDURES: Array<{
  fn: ProcedureFn<any, any, any>;
  paramsType: ProductType;
  returnType: AlgebraicType;
  returnTypeBaseSize: number;
}> = [];
