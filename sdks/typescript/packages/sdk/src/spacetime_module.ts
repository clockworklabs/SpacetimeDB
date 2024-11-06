import type { AlgebraicType } from './algebraic_type';
import type { DBConnectionImpl } from './db_connection_impl';
import type { Event } from './event';

export interface TableRuntimeTypeInfo {
  tableName: string;
  rowType: AlgebraicType;
  primaryKey?: string | undefined;
}

export interface ReducerRuntimeTypeInfo {
  reducerName: string;
  argsType: AlgebraicType;
}

export default interface SpacetimeModule {
  tables: { [name: string]: TableRuntimeTypeInfo };
  reducers: { [name: string]: ReducerRuntimeTypeInfo };
  eventContextConstructor: (imp: DBConnectionImpl, event: any) => any;
  dbViewConstructor: (connection: DBConnectionImpl) => any;
  reducersConstructor: (
    connection: DBConnectionImpl,
    setReducerFlags: any
  ) => any;
  setReducerFlagsConstructor: () => any;
}
