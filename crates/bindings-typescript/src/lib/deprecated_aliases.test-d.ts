import type {
  ClientTable,
  DbConnectionImpl,
  RemoteModule,
  RemoteModuleDecl,
  RemoteModuleDeclOf,
  RemoteModuleOf,
  TableDeclOf,
  TableDefByName,
  TableDefForTableName,
  TableHandle,
  TableNames,
  TableNamesOf,
  TypedTableDecl,
  TypedTableDef,
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
import type {
  UntypedProcedureDecl,
  UntypedProcedureDef,
  UntypedProceduresDecl,
  UntypedProceduresDef,
} from '../sdk/procedures';
import type {
  UntypedReducerDecl,
  UntypedReducerDef,
  UntypedReducersDecl,
  UntypedReducersDef,
} from '../sdk/reducers';
import type {
  ReducersDef,
  SchemaDef,
  UntypedRemoteModule,
  UntypedRemoteModuleDecl,
} from '../sdk/spacetime_module';
import type { IndexOpts } from './indexes';
import type {
  ModuleDef,
  RawModuleDefSections,
  SchemaDecl,
  TableDecl,
  TablesToSchema,
  TableToSchema,
  UntypedSchemaDecl,
  UntypedSchemaDef,
} from './schema';
import type { UntypedTableDecl, UntypedTableDef } from './table';
import type {
  ScheduleTableForParams,
  TableBody,
  TableSchedule,
  UntypedScheduledFunctionExport,
  UntypedTableBody,
} from './table_body';
import type * as table_schema from './table_schema';
import type { ColumnBuilder } from './type_builders';

type Equals<A, B> =
  (<T>() => T extends A ? 1 : 2) extends <T>() => T extends B ? 1 : 2
    ? true
    : false;
type Assert<T extends true> = T;

// Each deprecated alias must stay identical to the type that replaced it.
type _RemoteModule = Assert<
  Equals<
    RemoteModule<UntypedSchemaDecl, UntypedReducersDecl, UntypedProceduresDecl>,
    RemoteModuleDecl<
      UntypedSchemaDecl,
      UntypedReducersDecl,
      UntypedProceduresDecl
    >
  >
>;
type _RemoteModuleOf = Assert<
  Equals<
    RemoteModuleOf<
      DbConnectionImpl<
        RemoteModuleDecl<
          UntypedSchemaDecl,
          UntypedReducersDecl,
          UntypedProceduresDecl
        >
      >
    >,
    RemoteModuleDeclOf<
      DbConnectionImpl<
        RemoteModuleDecl<
          UntypedSchemaDecl,
          UntypedReducersDecl,
          UntypedProceduresDecl
        >
      >
    >
  >
>;
type _UntypedRemoteModule = Assert<
  Equals<UntypedRemoteModule, UntypedRemoteModuleDecl>
>;
type _SchemaDef = Assert<
  Equals<SchemaDef<UntypedRemoteModuleDecl>, UntypedRemoteModuleDecl['tables']>
>;
type _ReducersDef = Assert<
  Equals<
    ReducersDef<UntypedRemoteModuleDecl>,
    UntypedRemoteModuleDecl['reducers']
  >
>;
type _ModuleDef = Assert<Equals<ModuleDef, RawModuleDefSections>>;
type UntypedRow = Record<string, ColumnBuilder<any, any, any>>;
type UntypedIndexes = readonly IndexOpts<string>[];
// `lib/table_schema` was renamed to `lib/table_body`, but declaration files
// emitted against older versions can name the old path.
type _TableSchema = Assert<
  Equals<
    table_schema.TableSchema<UntypedRow, UntypedIndexes>,
    TableBody<UntypedRow, UntypedIndexes>
  >
>;
type _UntypedTableSchema = Assert<
  Equals<table_schema.UntypedTableSchema, UntypedTableBody>
>;
type _TableSchedule = Assert<Equals<table_schema.TableSchedule, TableSchedule>>;
type _ScheduleTableForParams = Assert<
  Equals<
    table_schema.ScheduleTableForParams<Record<string, any>>,
    ScheduleTableForParams<Record<string, any>>
  >
>;
type _UntypedScheduledFunctionExport = Assert<
  Equals<
    table_schema.UntypedScheduledFunctionExport,
    UntypedScheduledFunctionExport
  >
>;
type _TableToSchema = Assert<
  Equals<
    TableToSchema<'table', UntypedTableBody>,
    TableDecl<'table', UntypedTableBody>
  >
>;
type _TablesToSchema = Assert<
  Equals<
    TablesToSchema<Record<string, UntypedTableBody>>,
    SchemaDecl<Record<string, UntypedTableBody>>
  >
>;
type _UntypedTableDef = Assert<Equals<UntypedTableDef, UntypedTableDecl>>;
type _TypedTableDef = Assert<
  Equals<TypedTableDef<UntypedRow>, TypedTableDecl<UntypedRow>>
>;
type _UntypedSchemaDef = Assert<Equals<UntypedSchemaDef, UntypedSchemaDecl>>;
type _UntypedReducerDef = Assert<Equals<UntypedReducerDef, UntypedReducerDecl>>;
type _UntypedReducersDef = Assert<
  Equals<UntypedReducersDef, UntypedReducersDecl>
>;
type _UntypedProcedureDef = Assert<
  Equals<UntypedProcedureDef, UntypedProcedureDecl>
>;
type _UntypedProceduresDef = Assert<
  Equals<UntypedProceduresDef, UntypedProceduresDecl>
>;
type _TableNames = Assert<
  Equals<TableNames<UntypedSchemaDecl>, TableNamesOf<UntypedSchemaDecl>>
>;
type _TableDefByName = Assert<
  Equals<
    TableDefByName<UntypedSchemaDecl, string>,
    TableDeclOf<UntypedSchemaDecl, string>
  >
>;
type _TableDefForTableName = Assert<
  Equals<
    TableDefForTableName<UntypedSchemaDecl, string>,
    TableDeclOf<UntypedSchemaDecl, string>
  >
>;
type _ReducerCtx = Assert<
  Equals<ReducerCtx<UntypedSchemaDecl>, ReducerContext<UntypedSchemaDecl>>
>;
type _ProcedureCtx = Assert<
  Equals<ProcedureCtx<UntypedSchemaDecl>, ProcedureContext<UntypedSchemaDecl>>
>;
type _ViewCtx = Assert<
  Equals<ViewCtx<UntypedSchemaDecl>, ViewContext<UntypedSchemaDecl>>
>;
type _AnonymousViewCtx = Assert<
  Equals<
    AnonymousViewCtx<UntypedSchemaDecl>,
    AnonymousViewContext<UntypedSchemaDecl>
  >
>;
type _TransactionCtx = Assert<
  Equals<TransactionCtx<UntypedSchemaDecl>, TxContext<UntypedSchemaDecl>>
>;
type _AuthCtx = Assert<Equals<AuthCtx, AuthContext>>;
type _ClientTable = Assert<
  Equals<
    ClientTable<UntypedRemoteModuleDecl, TableNamesOf<UntypedRemoteModuleDecl>>,
    TableHandle<UntypedRemoteModuleDecl, TableNamesOf<UntypedRemoteModuleDecl>>
  >
>;
