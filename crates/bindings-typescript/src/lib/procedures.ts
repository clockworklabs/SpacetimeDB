import {
  AlgebraicType,
  ProductType,
  type Deserializer,
  type Serializer,
} from '../lib/algebraic_type';
import type { ConnectionId } from '../lib/connection_id';
import type { Identity } from '../lib/identity';
import type { Timestamp } from '../lib/timestamp';
import type { HttpClient } from '../server/http_internal';
import type { ParamsObj, ReducerCtx } from './reducers';
import {
  MODULE_DEF,
  registerTypesRecursively,
  type UntypedSchemaDef,
} from './schema';
import {
  type Infer,
  type InferTypeOfRow,
  type TypeBuilder,
} from './type_builders';
import type { CamelCase } from './type_util';
import {
  bsatnBaseSize,
  coerceParams,
  toCamelCase,
  type CoerceParams,
} from './util';
import type { Uuid } from './uuid';

export type ProcedureFn<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
> = (ctx: ProcedureCtx<S>, args: InferTypeOfRow<Params>) => Infer<Ret>;

export interface ProcedureCtx<S extends UntypedSchemaDef> {
  readonly sender: Identity;
  readonly identity: Identity;
  readonly timestamp: Timestamp;
  readonly connectionId: ConnectionId | null;
  readonly http: HttpClient;
  readonly counter_uuid: { value: number };
  withTx<T>(body: (ctx: TransactionCtx<S>) => T): T;
  newUuidV4(): Uuid;
  newUuidV7(): Uuid;
}

// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface TransactionCtx<S extends UntypedSchemaDef>
  extends ReducerCtx<S> {}

export function procedure<
  S extends UntypedSchemaDef,
  Params extends ParamsObj,
  Ret extends TypeBuilder<any, any>,
>(name: string, params: Params, ret: Ret, fn: ProcedureFn<S, Params, Ret>) {
  const paramsType: ProductType = {
    elements: Object.entries(params).map(([n, c]) => ({
      name: n,
      algebraicType: registerTypesRecursively(
        'typeBuilder' in c ? c.typeBuilder : c
      ).algebraicType,
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

  const { typespace } = MODULE_DEF;

  PROCEDURES.push({
    fn,
    deserializeArgs: ProductType.makeDeserializer(paramsType, typespace),
    serializeReturn: AlgebraicType.makeSerializer(returnType, typespace),
    returnTypeBaseSize: bsatnBaseSize(typespace, returnType),
  });
}

export const PROCEDURES: Array<{
  fn: ProcedureFn<any, any, any>;
  deserializeArgs: Deserializer<any>;
  serializeReturn: Serializer<any>;
  returnTypeBaseSize: number;
}> = [];

export type UntypedProcedureDef = {
  name: string;
  accessorName: string;
  params: CoerceParams<ParamsObj>;
  returnType: TypeBuilder<any, any>;
};

export type UntypedProceduresDef = {
  procedures: readonly UntypedProcedureDef[];
};

export function procedures<const H extends readonly UntypedProcedureDef[]>(
  ...handles: H
): { procedures: H };

export function procedures<const H extends readonly UntypedProcedureDef[]>(
  handles: H
): { procedures: H };

export function procedures<const H extends readonly UntypedProcedureDef[]>(
  ...args: [H] | H
): { procedures: H } {
  const procedures = (
    args.length === 1 && Array.isArray(args[0]) ? args[0] : args
  ) as H;
  return { procedures };
}

type ProcedureDef<
  Name extends string,
  Params extends ParamsObj,
  ReturnType extends TypeBuilder<any, any>,
> = {
  name: Name;
  accessorName: CamelCase<Name>;
  params: CoerceParams<Params>;
  returnType: ReturnType;
};

export function procedureSchema<
  ProcedureName extends string,
  Params extends ParamsObj,
  ReturnType extends TypeBuilder<any, any>,
>(
  name: ProcedureName,
  params: Params,
  returnType: ReturnType
): ProcedureDef<ProcedureName, Params, ReturnType> {
  return {
    name,
    accessorName: toCamelCase(name),
    params: coerceParams(params),
    returnType,
  };
}
