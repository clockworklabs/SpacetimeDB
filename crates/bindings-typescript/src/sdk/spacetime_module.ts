import type { UntypedProceduresDef } from './procedures';
import type { UntypedSchemaDef } from '../lib/schema';
import type { UntypedReducersDef } from './reducers';

export type ModuleDef<
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

/** @deprecated Use `ModuleDef` instead. */
export type RemoteModule<
  SchemaDef extends UntypedSchemaDef,
  ReducersDef extends UntypedReducersDef,
  ProceduresDef extends UntypedProceduresDef,
  CLI extends string = string,
> = ModuleDef<SchemaDef, ReducersDef, ProceduresDef, CLI>;

export type UntypedModuleDef = ModuleDef<
  UntypedSchemaDef,
  UntypedReducersDef,
  UntypedProceduresDef
>;
