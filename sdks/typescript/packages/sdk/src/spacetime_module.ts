import type { AlgebraicType } from '../../../../../crates/bindings-typescript/src/algebraic_type';
import type { DbConnectionImpl } from './db_connection_impl';

export interface TableRuntimeTypeInfo {
  tableName: string;
  rowType: AlgebraicType;
  primaryKeyInfo?: PrimaryKeyInfo;
}

export interface PrimaryKeyInfo {
  colName: string;
  colType: AlgebraicType;
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
  versionInfo?: {
    cliVersion: string;
  };
}
