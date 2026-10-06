import type { UntypedRemoteModuleDecl } from './spacetime_module';
import type { ClientTable } from './client_table';
import type { Values } from '../lib/type_util';

/**
 * A type representing a client-side database view, mapping table names to their corresponding client Table handles.
 */
export type ClientDbView<RemoteModuleDecl extends UntypedRemoteModuleDecl> = {
  readonly [TblName in Values<
    RemoteModuleDecl['tables']
  >['accessorName']]: ClientTable<RemoteModuleDecl, TblName>;
};
