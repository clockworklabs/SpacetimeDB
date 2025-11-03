import type { UntypedRemoteModule } from "./spacetime_module";
import type { ClientTable } from "./client_table";

/**
 * A type representing a client-side database view, mapping table names to their corresponding client Table handles.
 */
export type ClientDbView<RemoteModule extends UntypedRemoteModule> = {
  readonly [Tbl in RemoteModule['tables'][number] as Tbl['accessorName']]: ClientTable<RemoteModule, Tbl>;
};
