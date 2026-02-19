import type { ParamsObj } from '../lib/reducers';
import type { Infer, InferTypeOfRow, TypeBuilder } from '../lib/type_builders';
import type { CamelCase } from '../lib/type_util';
import { coerceParams, toCamelCase, type CoerceParams } from '../lib/util';
import type { UntypedRemoteModule } from './spacetime_module';

// Utility: detect 'any'
type IfAny<T, Y, N> = 0 extends 1 & T ? Y : N;

// Loose shape that allows all three families even when key names are unknown
type ProceduresViewLoose = {
  // call: camelCase(name)
  [k: string]: (params: any) => Promise<any>;
};

export type ProceduresView<RemoteModule> = IfAny<
  RemoteModule,
  ProceduresViewLoose,
  RemoteModule extends UntypedRemoteModule
    ? // x: camelCase(name)
      {
        [K in RemoteModule['procedures'][number] as CamelCase<
          K['accessorName']
        >]: (
          params: InferTypeOfRow<K['params']>
        ) => Promise<Infer<K['returnType']>>;
      }
    : never
>;

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
