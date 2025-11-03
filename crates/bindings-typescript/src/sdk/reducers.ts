import type { ProductType } from "../lib/algebraic_type";
import type { ParamsObj } from "../lib/reducers";
import type { CamelCase } from "../lib/type_util";
import type { CallReducerFlags } from "./db_connection_impl";

export type ReducersView<R extends UntypedReducersDef> = {
  [I in keyof R['reducers'] as CamelCase<R['reducers'][number]['name']>]:
    (params: R['reducers'][number]['params']) => void
};

export type UntypedReducers = Record<string, (...args: any[]) => void>;

export type UntypedReducerDef = {
  name: string;
  params: ParamsObj;
  paramsSpacetimeType: ProductType;
};

export type UntypedReducersDef = {
  reducers: readonly UntypedReducerDef[];
};

export type SetReducerFlags<R extends UntypedReducersDef> = {
  [K in keyof R['reducers'] as CamelCase<R['reducers'][number]['name']>]: (flags: CallReducerFlags) => void;
};