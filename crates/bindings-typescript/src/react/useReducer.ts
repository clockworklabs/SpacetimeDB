import type { DbConnectionImpl } from "../sdk/db_connection_impl";
import type { ReducerNamesFromReducers } from "../sdk/reducer_handle";
import { useSpacetimeDB } from "./useSpacetimeDB";

// export function useReducer<
//   DbConnection extends DbConnectionImpl,
//   ReducerName extends ReducerNamesFromReducers<DbConnection['reducers']> = ReducerNamesFromReducers<
//     DbConnection['reducers']
//   >,
//   ReducerType = DbConnection['reducers'][ReducerName & keyof DbConnection['reducers']]
// >(
//   reducerName: ReducerName,
// ): ReducerType {
//     const connectionState = useSpacetimeDB<DbConnection>();
//     const connection = connectionState.getConnection()!;
//     return connection.reducers[reducerName as keyof typeof connection.reducers];
// }