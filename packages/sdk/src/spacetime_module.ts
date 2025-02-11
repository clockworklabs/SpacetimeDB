import type { AlgebraicType } from './algebraic_type';
import type { DbConnectionImpl } from './db_connection_impl';

export interface TableRuntimeTypeInfo {
  tableName: string;
  rowType: AlgebraicType;
  primaryKey?: string | undefined;
}

export interface ReducerRuntimeTypeInfo {
  reducerName: string;
  argsType: AlgebraicType;
}

export default interface RemoteModule {
  tables: { [name: string]: TableRuntimeTypeInfo };
  reducers: { [name: string]: ReducerRuntimeTypeInfo };
  eventContextConstructor: (imp: DbConnectionImpl, event: any) => any;
  dbViewConstructor: (connection: DbConnectionImpl) => any;
  reducersConstructor: (
    connection: DbConnectionImpl,
    setReducerFlags: any
  ) => any;
  setReducerFlagsConstructor: () => any;
}
