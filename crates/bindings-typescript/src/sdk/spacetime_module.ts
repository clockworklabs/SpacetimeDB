import type { UntypedProceduresDef } from './procedures';
import type { UntypedSchemaDecl } from '../lib/schema';
import type { UntypedReducersDef } from './reducers';

export type RemoteModule<
  SchemaDecl extends UntypedSchemaDecl,
  ReducersDef extends UntypedReducersDef,
  ProceduresDef extends UntypedProceduresDef,
  CLI extends string = string,
> = SchemaDecl &
  ReducersDef &
  ProceduresDef & {
    versionInfo: {
      cliVersion: CLI;
    };
  };

export type UntypedRemoteModule = RemoteModule<
  UntypedSchemaDecl,
  UntypedReducersDef,
  UntypedProceduresDef
>;
