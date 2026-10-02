import type { UntypedRemoteModule } from './spacetime_module.ts';
import type { ClientTable } from './client_table.ts';
import type { Values } from '../lib/type_util.ts';

/**
 * A type representing a client-side database view, mapping table names to their corresponding client Table handles.
 */
export type ClientDbView<RemoteModule extends UntypedRemoteModule> = {
  readonly [TblName in Values<
    RemoteModule['tables']
  >['accessorName']]: ClientTable<RemoteModule, TblName>;
};
