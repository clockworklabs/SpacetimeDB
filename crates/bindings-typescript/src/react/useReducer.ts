import type { InferTypeOfRow } from "../lib/type_builders";
import type { PrettifyDeep } from "../lib/type_util";
import type { UntypedReducerDef } from "../sdk/reducers";
import { useSpacetimeDB, } from "./useSpacetimeDB";

export function useReducer<
  ReducerDef extends UntypedReducerDef
>(
  reducerDef: ReducerDef,
): (params: PrettifyDeep<InferTypeOfRow<ReducerDef['params']>>) => void {
    const reducerName = reducerDef.accessorName;
    const connectionState = useSpacetimeDB();
    const connection = connectionState.getConnection()!;
    return connection.reducers[reducerName as any] as (params: PrettifyDeep<InferTypeOfRow<ReducerDef['params']>>) => void;
}