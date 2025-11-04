import type { DbConnectionImpl, RemoteModuleOf } from "../sdk/db_connection_impl";
import type { ReducersView } from "../sdk/reducers";
import type { UntypedRemoteModule } from "../sdk/spacetime_module";
import { useSpacetimeDB, } from "./useSpacetimeDB";


export function useReducer<
  DbConnection extends DbConnectionImpl<UntypedRemoteModule>,
  ReducerName extends keyof ReducersView<RemoteModuleOf<DbConnection>> = keyof ReducersView<RemoteModuleOf<DbConnection>>
>(
  reducerName: ReducerName 
): ReducersView<RemoteModuleOf<DbConnection>>[ReducerName] {
    const connectionState = useSpacetimeDB<DbConnection>();
    const connection = connectionState.getConnection()!;
    return connection.reducers[reducerName];
}