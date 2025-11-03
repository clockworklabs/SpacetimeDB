import type { DbConnectionImpl } from "../sdk/db_connection_impl";
import type { ReducerNamesFromReducers } from "../sdk/reducer_handle";
import type { UntypedRemoteModule } from "../sdk/spacetime_module";
import { useSpacetimeDB } from "./useSpacetimeDB";

export function useReducer<
  RemoteModule extends UntypedRemoteModule,
  DbConnection extends DbConnectionImpl<RemoteModule>,
  ReducerName extends ReducerNamesFromReducers<DbConnection['reducers']> = ReducerNamesFromReducers<
    DbConnection['reducers']
  >,
  ReducerType = DbConnection['reducers'][ReducerName & keyof DbConnection['reducers']]
>(
  reducerName: ReducerName,
): ReducerType {
    const connectionState = useSpacetimeDB<RemoteModule, DbConnection>();
    const connection = connectionState.getConnection()!;
    return connection.reducers[reducerName as keyof typeof connection.reducers];
}