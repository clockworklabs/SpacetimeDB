import type {
  ClientTable,
  ModuleDef,
  RemoteModule,
  TableDefByName,
  TableDefForTableName,
  TableDefOf,
  TableHandle,
  TableNames,
  TableNamesOf,
} from '..';
import type {
  AnonymousViewContext,
  AnonymousViewCtx,
  AuthContext,
  AuthCtx,
  ProcedureContext,
  ProcedureCtx,
  ReducerContext,
  ReducerCtx,
  TransactionCtx,
  TxContext,
  ViewContext,
  ViewCtx,
} from '../server';
import type { UntypedProceduresDef } from '../sdk/procedures';
import type { UntypedReducersDef } from '../sdk/reducers';
import type { UntypedModuleDef } from '../sdk/spacetime_module';
import type { UntypedSchemaDef } from './schema';

type Equals<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;
type Assert<T extends true> = T;

// Each deprecated alias must stay identical to the type that replaced it.
type _RemoteModule = Assert<
  Equals<
    RemoteModule<UntypedSchemaDef, UntypedReducersDef, UntypedProceduresDef>,
    ModuleDef<UntypedSchemaDef, UntypedReducersDef, UntypedProceduresDef>
  >
>;
type _TableNames = Assert<
  Equals<TableNames<UntypedSchemaDef>, TableNamesOf<UntypedSchemaDef>>
>;
type _TableDefByName = Assert<
  Equals<
    TableDefByName<UntypedSchemaDef, string>,
    TableDefOf<UntypedSchemaDef, string>
  >
>;
type _TableDefForTableName = Assert<
  Equals<
    TableDefForTableName<UntypedSchemaDef, string>,
    TableDefOf<UntypedSchemaDef, string>
  >
>;
type _ReducerCtx = Assert<
  Equals<ReducerCtx<UntypedSchemaDef>, ReducerContext<UntypedSchemaDef>>
>;
type _ProcedureCtx = Assert<
  Equals<ProcedureCtx<UntypedSchemaDef>, ProcedureContext<UntypedSchemaDef>>
>;
type _ViewCtx = Assert<
  Equals<ViewCtx<UntypedSchemaDef>, ViewContext<UntypedSchemaDef>>
>;
type _AnonymousViewCtx = Assert<
  Equals<
    AnonymousViewCtx<UntypedSchemaDef>,
    AnonymousViewContext<UntypedSchemaDef>
  >
>;
type _TransactionCtx = Assert<
  Equals<TransactionCtx<UntypedSchemaDef>, TxContext<UntypedSchemaDef>>
>;
type _AuthCtx = Assert<Equals<AuthCtx, AuthContext>>;
type _ClientTable = Assert<
  Equals<
    ClientTable<UntypedModuleDef, TableNamesOf<UntypedModuleDef>>,
    TableHandle<UntypedModuleDef, TableNamesOf<UntypedModuleDef>>
  >
>;
