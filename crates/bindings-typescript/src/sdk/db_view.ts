import type { UntypedRemoteModule } from './spacetime_module';
import type { ClientTable } from './client_table';
import type { Values } from '../lib/type_util';

/**
 * A type representing a client-side database view, mapping table names to their corresponding client Table handles.
 */
export type ClientDbView<RemoteModule extends UntypedRemoteModule> = {
  readonly [TblName in Values<
    RemoteModule['tables']
  >['accessorName']]: ClientTable<RemoteModule, TblName>;
};
