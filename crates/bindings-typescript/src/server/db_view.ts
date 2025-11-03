import type { ClientTable } from "../sdk";
import type { UntypedRemoteModule } from "../sdk/spacetime_module";
import type { UntypedSchemaDef } from "./schema";
import type { ReadonlyTable, Table } from "./table";

/**
 * A type representing a read-only database view, mapping table names to their corresponding read-only Table handles.
 */
export type ReadonlyDbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['accessorName']]: ReadonlyTable<Tbl>;
};

/**
 * A type representing a client-side database view, mapping table names to their corresponding client Table handles.
 */
export type ClientDbView<RemoteModule extends UntypedRemoteModule> = {
  readonly [Tbl in RemoteModule['tables'][number] as Tbl['accessorName']]: ClientTable<RemoteModule, Tbl>;
};

/**
 * A type representing the database view, mapping table names to their corresponding Table handles.
 */
export type DbView<SchemaDef extends UntypedSchemaDef> = {
  readonly [Tbl in SchemaDef['tables'][number] as Tbl['accessorName']]: Table<Tbl>;
};