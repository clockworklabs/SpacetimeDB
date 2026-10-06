import type { Infer, InferTypeOfParams } from '.';
import type { Prettify } from '../lib/type_util';
import type { UntypedProcedureDecl } from './procedures';
import type { UntypedReducerDecl } from './reducers';

export type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
export type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

export type ParamsType<R extends UntypedReducerDecl> = MaybeParams<
  Prettify<InferTypeOfParams<R['params']>>
>;

export type ProcedureParamsType<P extends UntypedProcedureDecl> = MaybeParams<
  Prettify<InferTypeOfParams<P['params']>>
>;

export type ProcedureReturnType<P extends UntypedProcedureDecl> = Infer<
  P['returnType']
>;
