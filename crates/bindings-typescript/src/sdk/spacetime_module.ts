import type { UntypedSchemaDef } from '../server/schema';
import type { UntypedReducersDef } from './reducers';

export type RemoteModule<
  SchemaDef extends UntypedSchemaDef,
  ReducersDef extends UntypedReducersDef,
  CLI extends string = string
> = SchemaDef & ReducersDef & {
  versionInfo: {
    cliVersion: CLI;
  };
}

export type UntypedRemoteModule = RemoteModule<UntypedSchemaDef, UntypedReducersDef>;

export type SchemaDef<RemoteModule extends UntypedRemoteModule> = RemoteModule['tables'];

export type ReducersDef<RemoteModule extends UntypedRemoteModule> = RemoteModule['reducers'];