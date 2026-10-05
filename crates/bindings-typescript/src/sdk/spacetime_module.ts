import type { UntypedProceduresDecl } from './procedures';
import type { UntypedSchemaDecl } from '../lib/schema';
import type { UntypedReducersDecl } from './reducers';

export type RemoteModule<
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

export type UntypedRemoteModule = RemoteModule<
  UntypedSchemaDecl,
  UntypedReducersDecl,
  UntypedProceduresDecl
>;
