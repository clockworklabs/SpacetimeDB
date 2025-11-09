import type { ProductType } from "../lib/algebraic_type";
import type { ParamsObj } from "../lib/reducers";
import type { CoerceRow } from "../lib/table";
import type { InferTypeOfRow } from "../lib/type_builders";
import type { CamelCase } from "../lib/type_util";
import type { CallReducerFlags } from "./db_connection_impl";

export type ReducersView<R extends UntypedReducersDef> = {
  [I in keyof R['reducers'] as CamelCase<R['reducers'][number]['accessorName']>]:
    (params: InferTypeOfRow<R['reducers'][number]['params']>) => void
};

export type ReducerEventInfo<Args extends object = object> = {
  name: string;
  args: Args;
};

export type UntypedReducerDef = {
  name: string;
  accessorName: string;
  params: CoerceRow<ParamsObj>; 
  paramsType: ProductType;
};

export type UntypedReducersDef = {
  reducers: readonly UntypedReducerDef[];
};

export type SetReducerFlags<R extends UntypedReducersDef> = {
  [K in keyof R['reducers'] as CamelCase<R['reducers'][number]['name']>]: (flags: CallReducerFlags) => void;
};