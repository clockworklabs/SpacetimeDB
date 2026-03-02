import type { InferTypeOfRow } from '.';
import type { Prettify } from '../lib/type_util';
import type { UntypedReducerDef } from './reducers';

export type IsEmptyObject<T> = [keyof T] extends [never] ? true : false;
export type MaybeParams<T> = IsEmptyObject<T> extends true ? [] : [params: T];

export type ParamsType<R extends UntypedReducerDef> = MaybeParams<
  Prettify<InferTypeOfRow<R['params']>>
>;
