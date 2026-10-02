import type {
  DbConnectionImpl,
  RemoteModule,
  RemoteModuleDecl,
  RemoteModuleDeclOf,
  RemoteModuleOf,
  TableDeclOf,
  TableDefByName,
  TableDefForTableName,
  TableNames,
  TableNamesOf,
  TypedTableDecl,
  TypedTableDef,
} from '..';
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
  TableBody,
  TableSchema,
  UntypedTableBody,
  UntypedTableSchema,
} from './table_body';
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
type _ModuleDef = Assert<Equals<ModuleDef, RawModuleDefSections>>;
type UntypedRow = Record<string, ColumnBuilder<any, any, any>>;
type UntypedIndexes = readonly IndexOpts<string>[];
type _TableSchema = Assert<
  Equals<
    TableSchema<UntypedRow, UntypedIndexes>,
    TableBody<UntypedRow, UntypedIndexes>
  >
>;
type _UntypedTableSchema = Assert<Equals<UntypedTableSchema, UntypedTableBody>>;
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
