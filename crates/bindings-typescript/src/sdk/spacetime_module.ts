import type { UntypedProceduresDecl } from './procedures';
import type { UntypedSchemaDecl } from '../lib/schema';
import type { UntypedReducersDecl } from './reducers';

export type RemoteModuleDecl<
  SchemaDecl extends UntypedSchemaDecl,
  ReducersDecl extends UntypedReducersDecl,
  ProceduresDecl extends UntypedProceduresDecl,
  CLI extends string = string,
> = SchemaDecl &
  ReducersDecl &
  ProceduresDecl & {
    versionInfo: {
      cliVersion: CLI;
    };
  };

/** @deprecated Use `RemoteModuleDecl` instead. */
export type RemoteModule<
  SchemaDecl extends UntypedSchemaDecl,
  ReducersDecl extends UntypedReducersDecl,
  ProceduresDecl extends UntypedProceduresDecl,
  CLI extends string = string,
> = RemoteModuleDecl<SchemaDecl, ReducersDecl, ProceduresDecl, CLI>;

export type UntypedRemoteModuleDecl = RemoteModuleDecl<
  UntypedSchemaDecl,
  UntypedReducersDecl,
  UntypedProceduresDecl
>;

/** @deprecated Use `UntypedRemoteModuleDecl` instead. */
export type UntypedRemoteModule = UntypedRemoteModuleDecl;
