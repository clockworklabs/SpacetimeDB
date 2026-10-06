import type { Infer, InferTypeOfParams } from './index.ts';
import type { Prettify } from '../lib/type_util.ts';
import type { UntypedProcedureDef } from './procedures.ts';
import type { UntypedReducerDef } from './reducers.ts';

export type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
export type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

export type ParamsType<R extends UntypedReducerDef> = MaybeParams<
  Prettify<InferTypeOfParams<R['params']>>
>;

export type ProcedureParamsType<P extends UntypedProcedureDef> = MaybeParams<
  Prettify<InferTypeOfParams<P['params']>>
>;

export type ProcedureReturnType<P extends UntypedProcedureDef> = Infer<
  P['returnType']
>;
