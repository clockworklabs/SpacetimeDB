import type { UntypedRemoteModuleDecl } from './spacetime_module';
import type { TableHandle } from './client_table';
import type { Values } from '../lib/type_util';

/**
 * A type representing a client-side database view, mapping table names to their corresponding client Table handles.
 */
export type RemoteTables<RemoteModuleDecl extends UntypedRemoteModuleDecl> = {
  readonly [TblName in Values<
    RemoteModuleDecl['tables']
  >['accessorName']]: TableHandle<RemoteModuleDecl, TblName>;
};

/**
 * @deprecated Use `RemoteTables` instead. Kept so that declaration files
 * emitted against older versions of the SDK keep resolving.
 */
export type ClientDbView<RemoteModuleDecl extends UntypedRemoteModuleDecl> =
  RemoteTables<RemoteModuleDecl>;
