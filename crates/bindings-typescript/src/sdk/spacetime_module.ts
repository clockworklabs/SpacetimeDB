import type { UntypedProceduresDef } from './procedures';
import type { UntypedSchemaDef } from '../lib/schema';
import type { UntypedReducersDef } from './reducers';

export type RemoteModule<
  SchemaDef extends UntypedSchemaDef,
  ReducersDef extends UntypedReducersDef,
  ProceduresDef extends UntypedProceduresDef,
  CLI extends string = string,
> = SchemaDef &
  ReducersDef &
  ProceduresDef & {
    versionInfo: {
      cliVersion: CLI;
    };
  };

export type UntypedRemoteModule = RemoteModule<
  UntypedSchemaDef,
  UntypedReducersDef,
  UntypedProceduresDef
>;

export type SchemaDef<RemoteModule extends UntypedRemoteModule> =
  RemoteModule['tables'];

export type ReducersDef<RemoteModule extends UntypedRemoteModule> =
  RemoteModule['reducers'];
