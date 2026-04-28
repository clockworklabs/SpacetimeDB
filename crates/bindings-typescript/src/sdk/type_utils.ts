import type { Infer, InferTypeOfRow } from '.';
import type { Prettify } from '../lib/type_util';
import type { UntypedProcedureDef } from './procedures';
import type { UntypedReducerDef } from './reducers';

export type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
export type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

export type ParamsType<R extends UntypedReducerDef> = MaybeParams<
  Prettify<InferTypeOfRow<R['params']>>
>;

export type ProcedureParamsType<P extends UntypedProcedureDef> = MaybeParams<
  Prettify<InferTypeOfRow<P['params']>>
>;

export type ProcedureReturnType<P extends UntypedProcedureDef> = Infer<
  P['returnType']
>;
