import type { AlgebraicType } from '../';
import type { UntypedSchemaDef } from '../server/schema';
import type { DbConnectionImpl } from './db_connection_impl';
import type { UntypedReducersDef } from './reducers';

export interface TableRuntimeTypeInfo {
  tableName: string;
  rowType: AlgebraicType;
  primaryKey?: string;
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

export type RemoteModule2<SchemaDef extends UntypedSchemaDef, ReducersDef extends UntypedReducersDef, CLI extends string = string> = {
  versionInfo: {
    cliVersion: CLI;
  };
  tables: SchemaDef;
  reducers: ReducersDef;
}

export interface RemoteModule<SchemaDef extends UntypedSchemaDef, Reducers extends UntypedReducersDef> {
  tables: { [name: string]: TableRuntimeTypeInfo };
  reducers: { [name: string]: ReducerRuntimeTypeInfo };
  eventContextConstructor: (imp: DbConnectionImpl<SchemaDef, Reducers>, event: any) => any;
  dbViewConstructor: (connection: DbConnectionImpl<SchemaDef, Reducers>) => any;
  reducersConstructor: (
    connection: DbConnectionImpl<SchemaDef, Reducers>,
    setReducerFlags: any
  ) => any;
  setReducerFlagsConstructor: () => any;
  versionInfo?: {
    cliVersion: string;
  };
}
